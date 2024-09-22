use crate::view::ProtoNamespaceTemplate;
use askama::Template;
use mdbook::book::Chapter;
use std::collections::{BTreeMap, HashMap, HashSet};
use anyhow::{anyhow, Error, Result};
use pulldown_cmark::{CowStr, Event, Parser, Tag, TagEnd};
use pulldown_cmark_to_cmark::cmark;
use regex::Regex;

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

pub fn link_proto_symbols<'a, T>(chapter: &mut Chapter, links: &T) -> Result<()>
    where T: Iterator<Item=&'a SymbolLink> + Clone
{


    let re = Regex::new(r"proto!\((.*)\)").expect("should be valid regex");

    let mut buf = String::with_capacity(chapter.content.len());

    let events = Parser::new(&chapter.content).map(|e| {

        match e {
            Event::Start(Tag::Link {link_type,
                             dest_url,
                             title,
                             id}) if re.is_match(&dest_url) => {

                // let mut modified = e

                let Some(caps) = re.captures(&dest_url) else {
                    panic!("match with no capture!");
                };

                let link_query = &caps[1];

                let matches: Vec<_> = links.clone().filter(|&s| {
                    dbg!(&s.fqsl());
                    s.fqsl().contains(link_query)
                }).collect();

                let symbol_link = match matches.len() {
                    0 => {
                        panic!("No matches") // @todo show nearest match and offer it as a solution
                    }
                    1 => &matches[0],
                    _ => {
                        panic!("Too many matches") // @todo show nearest match and offer it as a solution
                    }
                };

                let new_dest = CowStr::Boxed(symbol_link.href().into());

                Event::Start(Tag::Link { link_type, dest_url: new_dest, title, id})
            }
            _ => e
        }

    });

    chapter.content = cmark(events, &mut buf).map(|_| buf).map_err(|err| {
        anyhow::Error::from(err)
    })?;

    Ok(())
}

#[cfg(test)]
mod test {
    use std::collections::{HashMap, HashSet};
    use mdbook::book::Chapter;
    use crate::links::{link_proto_symbols, SymbolLink};

    #[test]
    fn should_preserve_normal_links() {


        let mut chapter = Chapter {
            name: "".to_string(),
            content: r#"
# test chapter

Lorem ipsum [footnote link][1] [external link](https://example.com)

[1]: https://example.com

            "#.to_string(),
            number: None,
            sub_items: vec![],
            path: None,
            source_path: None,
            parent_names: vec![],
        };

        let original_content = chapter.content.clone();

        link_proto_symbols(&mut chapter, &[].into_iter()).expect("should succeed");

        assert_eq!(chapter.content.trim(), original_content.trim())
    }

    #[test]
    fn should_replace_proto_links_with_symbol_link() {

        let links = vec![SymbolLink::from_fqsl(".hello.HelloWorld".into(), &HashSet::from(["hello".into()]))];

        let mut chapter = Chapter {
            name: "".to_string(),
            content: r#"
# test chapter

Lorem ipsum [proto link](proto!(HelloWorld))

"#.to_string(),
            number: None,
            sub_items: vec![],
            path: None,
            source_path: None,
            parent_names: vec![],
        };

        link_proto_symbols(&mut chapter, &links.iter()).expect("should succeed");


        assert_eq!(chapter.content.trim(), r#"
# test chapter

Lorem ipsum [proto link](/proto/hello.md#HelloWorld)

"#.trim())

    }

}
