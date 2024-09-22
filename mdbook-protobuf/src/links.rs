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
    links: Vec<Backlink>,
}

impl Backlinks {
    pub(crate) fn new(links: Vec<Backlink>) -> Self {
        Self { links }
    }
}

#[derive(Clone)]
pub(crate) enum Backlink {
    Content(ContentLink),
    Symbol(SymbolLink),
}

#[derive(Template, Clone)]
#[template(path = "content_link.html")]
pub(crate) struct ContentLink {
    path: String,
    id: String,
    label: String,
}

impl ContentLink {
    fn href(&self) -> String {
        format!("/{}#{}", self.path, self.id)
    }
}

#[derive(Template, Clone, Eq, Hash, PartialEq)]
#[template(path = "symbol_link.html")]
pub(crate) struct SymbolLink {
    symbol: String,
    path: String,
    property: Option<String>,
    label_override: Option<String>,
    own_id: Option<String>,
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
                label_override: None,
                own_id: None,
            }
        } else {
            Self {
                symbol: fqsl_no_prop[1..].to_string(),
                path: "".into(),
                property,
                label_override: None,
                own_id: None,
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
        if let Some(label) = &self.label_override {
            return label.clone();
        }
        let fqsl = self.fqsl();
        if let Some(index) = fqsl.rfind('.') {
            fqsl[index + 1..].to_string()
        } else {
            fqsl
        }
    }

    fn set_label(&mut self, label: String) {
        self.label_override = Some(label)
    }

    fn href(&self) -> String {
        format!("/proto/{}.md#{}", self.path, self.id())
    }

    pub(crate) fn set_own_id(&mut self, id: String) {
        self.own_id = Some(id)
    }
}

pub fn assign_backlinks(
    document: &mut BTreeMap<String, ProtoNamespaceTemplate>,
    symbol_usages: HashMap<SymbolLink, Vec<Backlink>>,
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
    symbol_usages: &mut HashMap<SymbolLink, Vec<Backlink>>,
) -> Result<()> {
    let matcher = SkimMatcherV2::default();

    let mut chapter_link_id = 1;

    let links: Vec<_> = symbol_usages.keys().cloned().collect();

    // @todo assign symbol usages. maybe discriminate type with enum so they can be rendered differently.

    let re = Regex::new(r"proto!\((.*)\)").expect("should be valid regex");

    let mut buf = String::with_capacity(chapter.content.len());

    let mut current_link: Option<SymbolLink> = None;

    let events: Result<Vec<Event>> = Parser::new(&chapter.content).filter_map(|e| {

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

                let mut symbol_link = match matches.len() {
                    0 => {

                        let mut scored_links: Vec<_> = links.iter().map(|link|{
                            let fqsl= link.fqsl();

                            let distance = matcher.fuzzy_match(&fqsl, &link_query).unwrap_or(0);

                            (fqsl, distance)
                        })
                            .collect();

                        scored_links.sort_by_key(|(_, distance)|*distance);

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

                        return Some(Err(anyhow!(err_str)))
                    }
                    1 => matches[0],
                    _ => {

                        let replacements: Vec<_> = matches.iter().map(|&s|{
                            format!("proto!({})", s.fqsl())
                        }).collect();

                        let err_str = format!("More than one protobuf symbol matched your query. Replace your link with one of the following:\n{}", replacements.join("\n"));

                        return Some(Err(anyhow!(err_str)))
                    }
                }.clone();

                // don't backlink to draft chapters
                if let Some(path) = &chapter.path {

                    let current_usages_of_symbol = symbol_usages
                        .entry(symbol_link.clone())
                        .or_default();

                    let usage_id = chapter_link_id;

                    let id = format!("{}{}",&usage_id, symbol_link.fqsl());

                    symbol_link.set_own_id(id.clone());

                    let label = format!("{}[{}]", chapter.name, &usage_id);

                    let content_link = ContentLink { id, path: path.to_str().unwrap().to_string(), label};

                    current_usages_of_symbol.push(Backlink::Content(content_link))
                }

                current_link = Some(symbol_link);

                None
            }
            Event::Text(inner_text) if current_link.is_some() => {
                current_link.as_mut().expect("is some").set_label(inner_text.to_string());
                None
            },
            Event::End(TagEnd::Link) if current_link.is_some() => {

                let result = match current_link.as_ref().expect("is some").render() {
                    Ok(link_html) => {
                        let link = CowStr::Boxed(link_html.into());
                        Ok(Event::InlineHtml(link))
                    }
                    Err(e) => {
                        Err(anyhow!(e))
                    }
                };

                current_link = None;
                chapter_link_id += 1;

                Some(result)
            }
            _ => Some(Ok(e))
        }

    }).collect();

    chapter.content = cmark(events?.iter(), &mut buf)
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

<a href="foobar">inside</a>

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

        link_proto_symbols(&mut chapter, &mut Default::default()).expect("should succeed");

        assert_eq!(chapter.content.trim(), original_content.trim())
    }

    #[test]
    fn should_replace_proto_links_with_symbol_link() {
        let links = [(
            SymbolLink::from_fqsl(".hello.HelloWorld".into(), &HashSet::from(["hello".into()])),
            Default::default(),
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

        link_proto_symbols(&mut chapter, &mut HashMap::from(links)).expect("should succeed");

        assert_eq!(
            chapter.content.trim(),
            r#"
# test chapter

Lorem ipsum <a href="/proto/hello.md#HelloWorld">proto link</a>

"#
            .trim()
        )
    }

    #[test]
    fn should_error_and_offer_solutions_in_the_result_when_too_many_symbols_match() {
        let packages = HashSet::from(["hello".into(), "other".into()]);

        let links = [
            (
                SymbolLink::from_fqsl(".hello.HelloWorld".into(), &packages),
                Default::default(),
            ),
            (
                SymbolLink::from_fqsl(".other.HelloWorld".into(), &packages),
                Default::default(),
            ),
            (
                SymbolLink::from_fqsl(".other.Unrelated".into(), &packages),
                Default::default(),
            ),
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

        let res = link_proto_symbols(&mut chapter, &mut HashMap::from(links));

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

        let links = [
            (
                SymbolLink::from_fqsl(".hello.HelloWorld".into(), &packages),
                Default::default(),
            ),
            (
                SymbolLink::from_fqsl(".hello.GoodbyeWorld".into(), &packages),
                Default::default(),
            ),
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

        let res = link_proto_symbols(&mut chapter, &mut HashMap::from(links));

        assert_eq!(
            res.unwrap_err().to_string(),
            r#"No protobuf symbol matched your query `HelloWord`, consider one of the following near matches:
proto!(.hello.HelloWorld)"#
        )
    }

    #[test]
    fn should_link_to_parent_of_nested_message() {
        let packages = HashSet::from(["hello".into()]);
        let links = [
            (
                SymbolLink::from_fqsl(".hello.HelloWorld".into(), &packages),
                Default::default(),
            ),
            (
                SymbolLink::from_fqsl(".hello.HelloWorld.Nested".into(), &packages),
                Default::default(),
            ),
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

        link_proto_symbols(&mut chapter, &mut HashMap::from(links)).expect("should succeed");

        assert_eq!(
            chapter.content.trim(),
            r#"
# test chapter

Lorem ipsum <a href="/proto/hello.md#HelloWorld">proto link</a>

"#
            .trim()
        )
    }
}
