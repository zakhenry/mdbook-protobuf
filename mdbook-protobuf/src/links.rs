use crate::view::ProtoNamespaceTemplate;
use askama::Template;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) trait Linked {
    fn symbol_link(&self) -> &SymbolLink;
    fn fqsl(&self) -> String {
        self.symbol_link().fqsl()
    }

    fn set_backlinks(&mut self, backlinks: Backlinks);
}

#[derive(Template, Default)]
#[template(path = "backlinks.html")]
pub(crate) struct Backlinks {
    links: Vec<SymbolLink>,
}

impl Backlinks {
    pub(crate) fn new(links: Vec<SymbolLink>) -> Self {
        Self { links }
    }
}

#[derive(Template, Clone, Eq, Hash, PartialEq)]
#[template(path = "symbol_link.html")]
pub(crate) struct SymbolLink {
    symbol: String,
    path: String,
    property: Option<String>,
}

impl SymbolLink {
    pub(crate) fn set_property(&mut self, property: String) {
        self.property = Some(property)
    }
    // @todo refactor me, this function is a mess but write some tests first!
    pub(crate) fn from_fqsl(fqsl: String, packages: &HashSet<String>) -> Self {
        let (fqsl_no_prop, property) = if let Some((fqsl_no_prop, property)) = fqsl.split_once("::")
        {
            (fqsl_no_prop.to_string(), Some(property.to_string()))
        } else {
            (fqsl.clone(), None)
        };

        let best_match = packages
            .iter()
            .filter(|value| fqsl_no_prop[1..].starts_with(value.as_str()))
            .max_by_key(|value| value.len());

        if let Some(path) = best_match {
            Self {
                symbol: fqsl_no_prop[1..].replace(&format!("{}.", path), ""),
                path: path.to_string().replace(".", "/"),
                property,
            }
        } else {
            Self {
                symbol: fqsl_no_prop[1..].to_string(),
                path: "".into(),
                property,
            }
        }
    }

    pub(crate) fn id(&self) -> String {
        if let Some(property) = &self.property {
            format!("{}::{}", self.symbol, property)
        } else {
            self.symbol.to_string()
        }
    }

    fn fqsl(&self) -> String {
        format!(".{}.{}", self.path, self.id())
    }

    fn label(&self) -> String {
        let fqsl = self.fqsl();
        if let Some(index) = fqsl.rfind('.') {
            fqsl[index + 1..].to_string()
        } else {
            fqsl
        }
    }

    fn href(&self) -> String {
        format!("/proto/{}.md#{}", self.path, self.id())
    }
}

pub fn assign_backlinks(
    document: &mut BTreeMap<String, ProtoNamespaceTemplate>,
    symbol_usages: HashMap<SymbolLink, Vec<SymbolLink>>,
) {
    for (_, namespace) in document {
        namespace.mutate_symbols(|symbol| {
            if let Some(usages) = symbol_usages.get(&symbol.symbol_link()) {
                symbol.set_backlinks(Backlinks::new(usages.clone()))
            }
        })
    }
}
