use std::collections::{HashMap, HashSet};

use crate::{
    config,
    entry::{EntryMetaData, HTMLMetaData, MetaData, KEY_SLUG},
    slug,
};

use super::{
    callback::Callback,
    section::{HTMLContent, LazyContent, Section, SectionContent, SectionContents, ShallowSection},
    taxon::Taxon,
};

#[derive(Debug)]
pub struct CompileState {
    pub residued: HashMap<String, ShallowSection>,
    pub compiled: HashMap<String, Section>,
    pub metadata: HashMap<String, HTMLMetaData>,
    pub callback: Callback,
}

impl CompileState {
    pub fn new() -> CompileState {
        CompileState {
            residued: HashMap::new(),
            compiled: HashMap::new(),
            metadata: HashMap::new(),
            callback: Callback::new(),
        }
    }

    pub fn compile(&mut self, slug: &str) -> &Section {
        self.fetch_section(slug).unwrap()
    }

    pub fn compile_all(&mut self) {
        self.metadata = self
            .residued
            .iter_mut()
            .map(|(key, value)| {
                value.metadata.compute_textual_attrs();
                (key.to_string(), value.metadata.clone())
            })
            .collect();

        self.compile("index");
        /*
         * Unlinked or unembedded pages.
         */
        let residued_slugs: Vec<String> = self.residued.keys().map(|s| s.to_string()).collect();
        for slug in residued_slugs {
            self.compile(&slug);
        }
    }

    fn fetch_section(&mut self, slug: &str) -> Option<&Section> {
        if self.compiled.contains_key(slug) {
            return Some(self.compiled.get(slug).unwrap());
        }

        if self.residued.contains_key(slug) {
            let shallow = self.residued.remove(slug).unwrap();
            return Some(self.compile_shallow(shallow));
        }

        None // unreachable!("CompileState::fetch_section")
    }

    fn compile_shallow(&mut self, shallow: ShallowSection) -> &Section {
        let slug = shallow.slug();
        let mut children: SectionContents = vec![];
        let mut references: HashSet<String> = HashSet::new();

        match &shallow.content {
            HTMLContent::Plain(html) => {
                children.push(SectionContent::Plain(html.to_string()));
            }
            HTMLContent::Lazy(lazy_contents) => {
                let mut callback: Callback = Callback::new();

                for lazy_content in lazy_contents {
                    match lazy_content {
                        LazyContent::Plain(html) => {
                            children.push(SectionContent::Plain(html.to_string()));
                        }
                        LazyContent::Embed(embed_content) => {
                            let child_slug = slug::to_slug(&embed_content.url);
                            let refered = match self.fetch_section(&child_slug) {
                                Some(refered_section) => refered_section,
                                None => {
                                    eprintln!(
                                        "Error: [{}] attempting to fetch a non-existent [{}].",
                                        slug, child_slug,
                                    );
                                    continue;
                                }
                            };

                            if embed_content.option.details_open {
                                references.extend(refered.references.clone());
                            }
                            callback.insert_parent(child_slug, slug.to_string());

                            let mut child_section = refered.clone();
                            child_section.option = embed_content.option.clone();
                            if let Some(title) = &embed_content.title {
                                child_section
                                    .metadata
                                    .update("title".to_string(), title.to_string())
                            };
                            children.push(SectionContent::Embed(child_section));
                        }
                        LazyContent::Local(local_link) => {
                            let link_slug = &local_link.slug;
                            let article_title = self
                                .get_metadata(&link_slug)
                                .map_or("", |s| s.page_title().map_or("", |s| s));

                            if self.is_reference(&link_slug) {
                                references.insert(link_slug.to_string());
                            }

                            /*
                             * Making oneself the content of a backlink should not be expected behavior.
                             */
                            if *link_slug != slug
                                && format!("{}:metadata", link_slug) != slug
                                && self.is_enable_backlinks(&link_slug)
                            {
                                callback.insert_backlinks(
                                    link_slug.to_string(),
                                    vec![slug.to_string()],
                                );
                            }

                            let local_link = local_link.text.clone();
                            let text = local_link.unwrap_or(article_title.to_string());

                            let html = crate::html_flake::html_link(
                                &config::full_html_url(link_slug),
                                &format!("{} [{}]", article_title, link_slug),
                                &text,
                                crate::recorder::State::LocalLink.strify(),
                            );
                            children.push(SectionContent::Plain(html.to_string()));
                        }
                    }
                }

                self.callback.merge(callback);
            }
        };

        // compile metadata
        let mut metadata = EntryMetaData(HashMap::new());
        metadata.update(KEY_SLUG.to_string(), slug.to_string());
        shallow.metadata.keys().for_each(|key| {
            if key == KEY_SLUG {
                return;
            }
            let value = shallow.metadata.get(key).unwrap();
            let spanned: ShallowSection = Self::metadata_to_section(value, &slug);
            let compiled = self.compile_shallow(spanned);
            let html = compiled.spanned();
            metadata.update(key.to_string(), html);
        });

        // remove from `self.residued` after compiled.
        self.residued.remove(&slug);

        let section = Section::new(metadata, children, references);
        self.compiled.insert(slug.to_string(), section);
        self.compiled.get(&slug).unwrap()
    }

    pub fn metadata_to_section(content: &HTMLContent, current_slug: &str) -> ShallowSection {
        let mut metadata = HashMap::new();
        metadata.insert(
            KEY_SLUG.to_string(),
            HTMLContent::Plain(format!("{}:metadata", current_slug)),
        );

        return ShallowSection {
            metadata: HTMLMetaData(metadata),
            content: content.clone(),
        };
    }

    pub fn get_metadata(&self, slug: &str) -> Option<&HTMLMetaData> {
        self.metadata.get(slug)
    }

    pub fn is_enable_backlinks(&self, slug: &str) -> bool {
        self.metadata
            .get(slug)
            .map(|e| e.is_enable_backlinks())
            .unwrap_or(true)
    }

    pub fn is_reference(&self, slug: &str) -> bool {
        self.metadata
            .get(slug)
            .map(|e| e.is_asref() || Taxon::is_reference(e.data_taxon().map_or("", String::as_str)))
            .unwrap_or(false)
    }
}
