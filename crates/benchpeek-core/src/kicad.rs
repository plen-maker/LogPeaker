//! Parses a KiCad netlist (the s-expression `.net` file written by
//! Eeschema/KiCad's "Export Netlist", also embedded in `.kicad_pcb`) into
//! a list of nets and the component pins on each - enough to cross-highlight
//! a net against a live signal by name, without needing to render the
//! schematic or PCB itself.

use std::path::Path;

use anyhow::{anyhow, Result};

/// One component pin connected to a net.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNode {
    pub reference: String,
    pub pin: String,
}

/// One net, as KiCad named it, and every pin on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    pub name: String,
    pub nodes: Vec<NetNode>,
}

/// Every net extracted from a KiCad netlist file's `(nets ...)` section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetList {
    pub nets: Vec<Net>,
}

impl NetList {
    pub fn from_netlist_str(s: &str) -> Result<Self> {
        let root = SExpr::parse(s)?;
        Ok(Self::from_sexpr(&root))
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_netlist_str(&std::fs::read_to_string(path)?)
    }

    fn from_sexpr(root: &SExpr) -> Self {
        let nets = root
            .find_list("nets")
            .map(|nets_list| {
                nets_list
                    .children_lists("net")
                    .map(|net_expr| Net {
                        name: net_expr.find_string("name").unwrap_or_default(),
                        nodes: net_expr
                            .children_lists("node")
                            .map(|node_expr| NetNode {
                                reference: node_expr.find_string("ref").unwrap_or_default(),
                                pin: node_expr.find_string("pin").unwrap_or_default(),
                            })
                            .collect(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { nets }
    }
}

/// A minimal s-expression tree: just enough of KiCad's dialect (nested
/// parenthesized lists, quoted and bare atoms, `\"`-escaped quotes inside
/// strings) to walk a netlist - not a general Lisp reader.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SExpr {
    List(Vec<SExpr>),
    Atom(String),
}

impl SExpr {
    fn parse(input: &str) -> Result<SExpr> {
        let mut chars = input.chars().peekable();
        Self::parse_expr(&mut chars).ok_or_else(|| anyhow!("empty or invalid s-expression"))
    }

    fn parse_expr(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<SExpr> {
        Self::skip_ws(chars);
        match *chars.peek()? {
            '(' => {
                chars.next();
                let mut items = Vec::new();
                loop {
                    Self::skip_ws(chars);
                    match chars.peek() {
                        Some(')') => {
                            chars.next();
                            break;
                        }
                        Some(_) => items.push(Self::parse_expr(chars)?),
                        None => break, // unterminated input: tolerate rather than fail
                    }
                }
                Some(SExpr::List(items))
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                s.push(escaped);
                            }
                        }
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                Some(SExpr::Atom(s))
            }
            _ => {
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '(' || c == ')' {
                        break;
                    }
                    s.push(c);
                    chars.next();
                }
                Some(SExpr::Atom(s))
            }
        }
    }

    fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars>) {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
    }

    fn tag(&self) -> Option<&str> {
        match self {
            SExpr::List(items) => match items.first() {
                Some(SExpr::Atom(a)) => Some(a),
                _ => None,
            },
            SExpr::Atom(_) => None,
        }
    }

    /// Direct child lists tagged by their own first atom, e.g. `net` for
    /// `(net (name "VBAT") ...)`. Atoms and untagged lists are skipped.
    fn children_lists<'a, 'b>(&'a self, tag: &'b str) -> impl Iterator<Item = &'a SExpr> + 'b
    where
        'a: 'b,
    {
        let items: &[SExpr] = match self {
            SExpr::List(items) => items,
            SExpr::Atom(_) => &[],
        };
        items.iter().filter(move |e| e.tag() == Some(tag))
    }

    fn find_list(&self, tag: &str) -> Option<&SExpr> {
        self.children_lists(tag).next()
    }

    /// For a `(tag "value")` child list, its value as a string.
    fn find_string(&self, tag: &str) -> Option<String> {
        match self.find_list(tag)? {
            SExpr::List(items) => match items.get(1) {
                Some(SExpr::Atom(v)) => Some(v.clone()),
                _ => None,
            },
            SExpr::Atom(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
        (export (version "E")
          (design
            (source "/home/user/project.kicad_sch")
            (date "2024-01-01T00:00:00")
            (tool "Eeschema 7.0.0"))
          (components
            (comp (ref "U1")
              (value "STM32MP257F")
              (footprint "Package:LQFP144"))
            (comp (ref "R1")
              (value "10k")
              (footprint "Resistor_SMD:R_0603")))
          (nets
            (net (code "1") (name "VBAT")
              (node (ref "U1") (pin "3") (pinfunction "VDD"))
              (node (ref "R1") (pin "1")))
            (net (code "2") (name "GND")
              (node (ref "U1") (pin "1"))
              (node (ref "R1") (pin "2")))
            (net (code "3") (name "unconnected-(U1-Pad5)")
              (node (ref "U1") (pin "5")))))
    "#;

    #[test]
    fn parses_nets_and_nodes() {
        let nl = NetList::from_netlist_str(EXAMPLE).unwrap();
        assert_eq!(nl.nets.len(), 3);

        let vbat = &nl.nets[0];
        assert_eq!(vbat.name, "VBAT");
        assert_eq!(vbat.nodes.len(), 2);
        assert_eq!(vbat.nodes[0].reference, "U1");
        assert_eq!(vbat.nodes[0].pin, "3");
        assert_eq!(vbat.nodes[1].reference, "R1");
        assert_eq!(vbat.nodes[1].pin, "1");

        assert_eq!(nl.nets[1].name, "GND");
        assert_eq!(nl.nets[2].name, "unconnected-(U1-Pad5)");
    }

    #[test]
    fn missing_nets_section_yields_no_nets() {
        let nl = NetList::from_netlist_str(r#"(export (version "E") (design))"#).unwrap();
        assert!(nl.nets.is_empty());
    }

    #[test]
    fn tolerates_trailing_whitespace_and_unterminated_input() {
        assert!(NetList::from_netlist_str("   \n\t  ").is_err());
        let nl = NetList::from_netlist_str("(export (nets (net (name \"X\")").unwrap();
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "X");
    }
}
