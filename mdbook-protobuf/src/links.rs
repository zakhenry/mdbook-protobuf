use crate::view::ProtoNamespaceTemplate;
use anyhow::{anyhow, Error, Result};
use askama::Template;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use mdbook::book::Chapter;
use pulldown_cmark::{CowStr, Event, Parser, Tag, TagEnd};
use pulldown_cmark_to_cmark::cmark;
use regex::Regex;
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

pub fn link_proto_symbols(
    chapter: &mut Chapter,
    links: &[SymbolLink],
    symbol_usages: &mut HashMap<SymbolLink, Vec<SymbolLink>>,
) -> Result<()> {
    let matcher = SkimMatcherV2::default();

    // @todo assign symbol usages. maybe discriminate type with enum so they can be rendered differently.

    let re = Regex::new(r"proto!\((.*)\)").expect("should be valid regex");

    let mut buf = String::with_capacity(chapter.content.len());

    let events: Result<Vec<Event>> = Parser::new(&chapter.content).map(|e| {

        match e {
            Event::Start(Tag::Link {link_type,
                             dest_url,
                             title,
                             id}) if re.is_match(&dest_url) => {

                let Some(caps) = re.captures(&dest_url) else {
                    panic!("match with no capture!");
                };

                let link_query = &caps[1];

                let matches: Vec<_> = links.iter().filter(|&s| {
                    s.fqsl().ends_with(link_query)
                }).collect();

                let symbol_link = match matches.len() {
                    0 => {

                        let mut scored_links: Vec<_> = links.iter().map(|link|{
                            let fqsl= link.fqsl();

                            let distance = matcher.fuzzy_match(&fqsl, &link_query).unwrap_or(0);

                            (fqsl, distance)
                        })
                            .collect();

                        scored_links.sort_by_key(|(_, distance)|*distance);

                        dbg!(&scored_links);

                        let suggestions: Vec<_> = scored_links
                            .iter()
                            .rev()
                            .filter(|(_, distance)|*distance > 0)
                            .take(3)
                            .map(|(fqsl, _)|{
                            format!("proto!({})", &fqsl)
                        })
                            .collect();

                        let err_str = if suggestions.is_empty() {
                            let random_sample: Vec<_> = scored_links.iter().map(|((fqsl,_))|format!("proto!({})", &fqsl)).take(3).collect();
                            format!("No protobuf symbol matched your query `{}`, or was similar. Sample of valid formats:\n{}", &link_query, random_sample.join("\n"))
                        } else {
                            format!("No protobuf symbol matched your query `{}`, consider one of the following near matches:\n{}", &link_query, suggestions.join("\n"))
                        };

                        return Err(anyhow!(err_str))
                    }
                    1 => &matches[0],
                    _ => {

                        let replacements: Vec<_> = matches.iter().map(|&s|{
                            format!("proto!({})", s.fqsl())
                        }).collect();

                        let err_str = format!("More than one protobuf symbol matched your query. Replace your link with one of the following:\n{}", replacements.join("\n"));

                        return Err(anyhow!(err_str))
                    }
                };

                let new_dest = CowStr::Boxed(symbol_link.href().into());

                Ok::<Event<'_>, Error>(Event::Start(Tag::Link { link_type, dest_url: new_dest, title, id}))
            }
            _ => Ok(e)
        }

    }).collect();

    let events = events?;

    chapter.content = cmark(events.iter(), &mut buf)
        .map(|_| buf)
        .map_err(|err| anyhow::Error::from(err))?;

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::links::{link_proto_symbols, SymbolLink};
    use mdbook::book::Chapter;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn should_preserve_normal_links() {
        let mut chapter = Chapter {
            name: "".to_string(),
            content: r#"
# test chapter

Lorem ipsum [footnote link][1] [external link](https://example.com)

[1]: https://example.com

            "#
            .to_string(),
            number: None,
            sub_items: vec![],
            path: None,
            source_path: None,
            parent_names: vec![],
        };

        let original_content = chapter.content.clone();

        link_proto_symbols(&mut chapter, &[], &mut Default::default()).expect("should succeed");

        assert_eq!(chapter.content.trim(), original_content.trim())
    }

    #[test]
    fn should_replace_proto_links_with_symbol_link() {
        let links = vec![SymbolLink::from_fqsl(
            ".hello.HelloWorld".into(),
            &HashSet::from(["hello".into()]),
        )];

        let mut chapter = Chapter {
            name: "".to_string(),
            content: r#"
# test chapter

Lorem ipsum [proto link](proto!(HelloWorld))

"#
            .to_string(),
            number: None,
            sub_items: vec![],
            path: None,
            source_path: None,
            parent_names: vec![],
        };

        link_proto_symbols(&mut chapter, &links, &mut Default::default()).expect("should succeed");

        assert_eq!(
            chapter.content.trim(),
            r#"
# test chapter

Lorem ipsum [proto link](/proto/hello.md#HelloWorld)

"#
            .trim()
        )
    }

    #[test]
    fn should_error_and_offer_solutions_in_the_result_when_too_many_symbols_match() {
        let packages = HashSet::from(["hello".into(), "other".into()]);

        let links = vec![
            SymbolLink::from_fqsl(".hello.HelloWorld".into(), &packages),
            SymbolLink::from_fqsl(".other.HelloWorld".into(), &packages),
            SymbolLink::from_fqsl(".other.Unrelated".into(), &packages),
        ];

        let mut chapter = Chapter {
            name: "".to_string(),
            content: r#"
# test chapter

Lorem ipsum [proto link](proto!(HelloWorld))

"#
            .to_string(),
            number: None,
            sub_items: vec![],
            path: None,
            source_path: None,
            parent_names: vec![],
        };

        let res = link_proto_symbols(&mut chapter, &links, &mut Default::default());

        assert_eq!(
            res.unwrap_err().to_string(),
            r#"More than one protobuf symbol matched your query. Replace your link with one of the following:
proto!(.hello.HelloWorld)
proto!(.other.HelloWorld)"#
        )
    }

    #[test]
    fn should_error_and_offer_solutions_in_the_result_when_zero_symbols_match() {
        let packages = HashSet::from(["hello".into(), "other".into()]);

        let links = vec![
            SymbolLink::from_fqsl(".hello.HelloWorld".into(), &packages),
            SymbolLink::from_fqsl(".hello.GoodbyeWorld".into(), &packages),
        ];

        let mut chapter = Chapter {
            name: "".to_string(),
            content: r#"
# test chapter

Lorem ipsum [proto link](proto!(HelloWord))

"#
            .to_string(),
            number: None,
            sub_items: vec![],
            path: None,
            source_path: None,
            parent_names: vec![],
        };

        let res = link_proto_symbols(&mut chapter, &links, &mut Default::default());

        assert_eq!(
            res.unwrap_err().to_string(),
            r#"No protobuf symbol matched your query `HelloWord`, consider one of the following near matches:
proto!(.hello.HelloWorld)"#
        )
    }


    #[test]
    fn should_link_to_parent_of_nested_message() {
        let packages = HashSet::from(["hello".into()]);
        let links = vec![
            SymbolLink::from_fqsl(".hello.HelloWorld".into(), &packages),
            SymbolLink::from_fqsl(".hello.HelloWorld.Nested".into(), &packages),
        ];

        let mut chapter = Chapter {
            name: "".to_string(),
            content: r#"
# test chapter

Lorem ipsum [proto link](proto!(HelloWorld))

"#
                .to_string(),
            number: None,
            sub_items: vec![],
            path: None,
            source_path: None,
            parent_names: vec![],
        };

        link_proto_symbols(&mut chapter, &links, &mut Default::default()).expect("should succeed");

        assert_eq!(
            chapter.content.trim(),
            r#"
# test chapter

Lorem ipsum [proto link](/proto/hello.md#HelloWorld)

"#
                .trim()
        )
    }
}
