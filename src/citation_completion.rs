use glib::subclass::types::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use sourceview5::prelude::*;
use sourceview5::{CompletionCell, CompletionColumn, CompletionContext, View};
use sourceview5::subclass::prelude::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CitationEntry {
    key: String,
    title: String,
    source: String,
}

fn strip_quotes(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn parse_inline_list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(strip_quotes)
        .filter(|item| !item.is_empty())
        .collect()
}

fn bibliography_paths_from_markdown(markdown: &str) -> Vec<String> {
    let mut lines = markdown.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Vec::new();
    }

    let mut bibliography = Vec::new();
    let mut in_block = false;
    let mut block_indent = 0_usize;

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            break;
        }

        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        if in_block {
            if trimmed.is_empty() {
                continue;
            }
            if indent <= block_indent && !trimmed.starts_with('-') {
                in_block = false;
            } else if let Some(item) = trimmed.strip_prefix("- ") {
                let path = strip_quotes(item);
                if !path.is_empty() {
                    bibliography.push(path);
                }
                continue;
            } else {
                continue;
            }
        }

        if let Some(rest) = trimmed.strip_prefix("bibliography:") {
            let rest = rest.trim();
            if rest.is_empty() {
                in_block = true;
                block_indent = indent;
            } else if rest.starts_with('[') && rest.ends_with(']') {
                bibliography.extend(parse_inline_list(rest));
            } else {
                let path = strip_quotes(rest);
                if !path.is_empty() {
                    bibliography.push(path);
                }
            }
        }
    }

    bibliography
}

fn bibliography_files(markdown: &str, current_file: Option<&Path>) -> Vec<PathBuf> {
    let Some(base_dir) = current_file.and_then(Path::parent) else {
        return Vec::new();
    };

    let mut files: Vec<PathBuf> = bibliography_paths_from_markdown(markdown)
        .into_iter()
        .map(|path| base_dir.join(path))
        .collect();

    if !files.is_empty() {
        files.sort();
        files.dedup();
        return files;
    }

    let Ok(entries) = std::fs::read_dir(base_dir) else {
        return Vec::new();
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("bib") {
            files.push(path);
        }
    }

    files.sort();
    files.dedup();
    files
}

fn split_bibtex_entries(contents: &str) -> Vec<String> {
    let chars: Vec<char> = contents.chars().collect();
    let mut entries = Vec::new();
    let mut index = 0_usize;

    while index < chars.len() {
        if chars[index] != '@' {
            index += 1;
            continue;
        }

        let start = index;
        let mut cursor = index + 1;
        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        while cursor < chars.len() && chars[cursor].is_ascii_alphabetic() {
            cursor += 1;
        }
        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        if cursor >= chars.len() || !matches!(chars[cursor], '{' | '(') {
            index += 1;
            continue;
        }

        let mut depth = 1_i32;
        cursor += 1;
        let mut in_quotes = false;
        let mut escaped = false;

        while cursor < chars.len() {
            let ch = chars[cursor];
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_quotes = !in_quotes;
            } else if !in_quotes {
                if matches!(ch, '{' | '(') {
                    depth += 1;
                } else if matches!(ch, '}' | ')') {
                    depth -= 1;
                    if depth == 0 {
                        cursor += 1;
                        break;
                    }
                }
            }
            cursor += 1;
        }

        entries.push(chars[start..cursor.min(chars.len())].iter().collect());
        index = cursor;
    }

    entries
}

fn parse_entry_key(entry: &str) -> Option<String> {
    let trimmed = entry.trim_start();
    if !trimmed.starts_with('@') {
        return None;
    }

    let open_idx = trimmed.find(['{', '('])?;
    let entry_type = trimmed[1..open_idx].trim().to_ascii_lowercase();
    if matches!(entry_type.as_str(), "comment" | "preamble" | "string") {
        return None;
    }

    let rest = &trimmed[open_idx + 1..];
    let comma_idx = rest.find(',')?;
    let key = rest[..comma_idx].trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_bibtex_field(entry: &str, field: &str) -> Option<String> {
    let body_start = entry.find(',')? + 1;
    let chars: Vec<char> = entry[body_start..].chars().collect();
    let field = field.to_ascii_lowercase();
    let mut index = 0_usize;
    let mut depth = 1_i32;
    let mut in_quotes = false;
    let mut escaped = false;

    while index < chars.len() {
        let ch = chars[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += 1;
            continue;
        }
        if ch == '"' {
            in_quotes = !in_quotes;
            index += 1;
            continue;
        }
        if !in_quotes {
            if matches!(ch, '{' | '(') {
                depth += 1;
                index += 1;
                continue;
            }
            if matches!(ch, '}' | ')') {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
                index += 1;
                continue;
            }
        }

        if depth == 1 && ch.is_ascii_alphabetic() {
            let ident_start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || matches!(chars[index], '_' | '-'))
            {
                index += 1;
            }

            let ident: String = chars[ident_start..index].iter().collect();
            let ident = ident.to_ascii_lowercase();
            let mut cursor = index;
            while cursor < chars.len() && chars[cursor].is_whitespace() {
                cursor += 1;
            }
            if ident != field || cursor >= chars.len() || chars[cursor] != '=' {
                continue;
            }

            cursor += 1;
            while cursor < chars.len() && chars[cursor].is_whitespace() {
                cursor += 1;
            }
            if cursor >= chars.len() {
                return None;
            }

            if chars[cursor] == '{' {
                let mut value = String::new();
                let mut inner_depth = 1_i32;
                cursor += 1;
                while cursor < chars.len() {
                    let current = chars[cursor];
                    if current == '{' {
                        inner_depth += 1;
                        value.push(current);
                    } else if current == '}' {
                        inner_depth -= 1;
                        if inner_depth == 0 {
                            break;
                        }
                        value.push(current);
                    } else {
                        value.push(current);
                    }
                    cursor += 1;
                }
                let value = collapse_whitespace(&value);
                return (!value.is_empty()).then_some(value);
            }

            if chars[cursor] == '"' {
                let mut value = String::new();
                let mut escaped = false;
                cursor += 1;
                while cursor < chars.len() {
                    let current = chars[cursor];
                    if escaped {
                        value.push(current);
                        escaped = false;
                    } else if current == '\\' {
                        escaped = true;
                    } else if current == '"' {
                        break;
                    } else {
                        value.push(current);
                    }
                    cursor += 1;
                }
                let value = collapse_whitespace(&value);
                return (!value.is_empty()).then_some(value);
            }

            let start = cursor;
            while cursor < chars.len() && !matches!(chars[cursor], ',' | '\n' | '\r' | '}' | ')') {
                cursor += 1;
            }
            let value: String = chars[start..cursor].iter().collect();
            let value = collapse_whitespace(&value);
            return (!value.is_empty()).then_some(value);
        }

        index += 1;
    }

    None
}

fn bibtex_entries(contents: &str) -> Vec<(String, Option<String>)> {
    split_bibtex_entries(contents)
        .into_iter()
        .filter_map(|entry| {
            let key = parse_entry_key(&entry)?;
            let title = parse_bibtex_field(&entry, "title");
            Some((key, title))
        })
        .collect()
}

fn elide(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }

    let truncated: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}...")
}

fn citation_entries(markdown: &str, current_file: Option<&Path>) -> Vec<CitationEntry> {
    let mut entries = Vec::new();

    for bib_path in bibliography_files(markdown, current_file) {
        let Ok(contents) = std::fs::read_to_string(&bib_path) else {
            continue;
        };
        let source = bib_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("bibliography")
            .to_string();

        for (key, title) in bibtex_entries(&contents) {
            entries.push(CitationEntry {
                key,
                title: title.unwrap_or_else(|| source.clone()),
                source: source.clone(),
            });
        }
    }

    entries.sort_by(|left, right| left.key.cmp(&right.key));
    entries.dedup_by(|left, right| left.key == right.key);
    entries
}

fn is_citation_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.' | '/')
}

fn citation_query_at_cursor(buffer: &impl IsA<gtk4::TextBuffer>) -> Option<(gtk4::TextIter, String)> {
    let buffer = buffer.as_ref();
    let insert = buffer.get_insert();
    let cursor = buffer.iter_at_mark(&insert);
    let mut line_start = cursor;
    line_start.set_line_offset(0);

    let line_text = buffer.text(&line_start, &cursor, false).to_string();
    let chars: Vec<char> = line_text.chars().collect();
    let mut start = chars.len();
    while start > 0 && is_citation_key_char(chars[start - 1]) {
        start -= 1;
    }

    if start == 0 || chars[start - 1] != '@' {
        return None;
    }

    let prefix: String = chars[start..].iter().collect();
    let mut begin = cursor;
    begin.backward_chars(prefix.chars().count() as i32);
    Some((begin, prefix))
}

fn normalized(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn matching_entries(entries: &[CitationEntry], prefix: &str) -> Vec<CitationEntry> {
    let query = normalized(prefix);
    let mut matches: Vec<CitationEntry> = entries
        .iter()
        .filter(|entry| {
            if query.is_empty() {
                return true;
            }

            let key = normalized(&entry.key);
            key.starts_with(&query) || key.contains(&query)
        })
        .cloned()
        .collect();

    matches.sort_by(|left, right| {
        let left_key = normalized(&left.key);
        let right_key = normalized(&right.key);
        let left_prefix = !query.is_empty() && left_key.starts_with(&query);
        let right_prefix = !query.is_empty() && right_key.starts_with(&query);
        right_prefix
            .cmp(&left_prefix)
            .then_with(|| left.key.cmp(&right.key))
    });
    matches.truncate(50);
    matches
}

mod proposal_imp {
    use super::*;
    use glib::subclass::prelude::*;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::CitationCompletionProposal)]
    pub struct CitationCompletionProposal {
        #[property(get, construct_only)]
        pub key: RefCell<String>,
        #[property(get, construct_only)]
        pub title: RefCell<String>,
        #[property(get, construct_only)]
        pub details: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CitationCompletionProposal {
        const NAME: &'static str = "AgenticMdCitationCompletionProposal";
        type Type = super::CitationCompletionProposal;
        type ParentType = glib::Object;
        type Interfaces = (sourceview5::CompletionProposal,);
    }

    #[glib::derived_properties]
    impl ObjectImpl for CitationCompletionProposal {}

    impl CompletionProposalImpl for CitationCompletionProposal {}
}

glib::wrapper! {
    pub struct CitationCompletionProposal(ObjectSubclass<proposal_imp::CitationCompletionProposal>)
        @implements sourceview5::CompletionProposal;
}

impl CitationCompletionProposal {
    fn new(key: &str, title: &str, details: &str) -> Self {
        glib::Object::builder()
            .property("key", key)
            .property("title", title)
            .property("details", details)
            .build()
    }
}

mod provider_imp {
    use super::*;
    use glib::subclass::prelude::*;

    #[derive(Default)]
    pub struct CitationCompletionProvider {
        pub(super) entries: RefCell<Vec<CitationEntry>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CitationCompletionProvider {
        const NAME: &'static str = "AgenticMdCitationCompletionProvider";
        type Type = super::CitationCompletionProvider;
        type ParentType = glib::Object;
        type Interfaces = (sourceview5::CompletionProvider,);
    }

    impl ObjectImpl for CitationCompletionProvider {}

    impl CompletionProviderImpl for CitationCompletionProvider {
        fn activate(&self, context: &CompletionContext, proposal: &sourceview5::CompletionProposal) {
            let Some(proposal) = proposal.downcast_ref::<super::CitationCompletionProposal>() else {
                return;
            };
            let Some(buffer) = context.buffer() else {
                return;
            };
            let Some((mut begin, mut end)) = citation_query_at_cursor(&buffer)
                .map(|(begin, _)| (begin, buffer.iter_at_mark(&buffer.get_insert())))
            else {
                return;
            };

            buffer.begin_user_action();
            buffer.delete(&mut begin, &mut end);
            buffer.insert(&mut begin, &proposal.key());
            buffer.end_user_action();
        }

        fn display(
            &self,
            _context: &CompletionContext,
            proposal: &sourceview5::CompletionProposal,
            cell: &CompletionCell,
        ) {
            let Some(proposal) = proposal.downcast_ref::<super::CitationCompletionProposal>() else {
                return;
            };

            match cell.column() {
                CompletionColumn::TypedText => cell.set_text(Some(&proposal.key())),
                CompletionColumn::Comment => cell.set_text(Some(&proposal.title())),
                CompletionColumn::Details => cell.set_text(Some(&proposal.details())),
                _ => {}
            }
        }

        fn title(&self) -> Option<glib::GString> {
            Some("Citations".into())
        }

        fn priority(&self, _context: &CompletionContext) -> i32 {
            100
        }

        fn is_trigger(&self, _iter: &gtk4::TextIter, c: char) -> bool {
            c == '@'
        }

        fn populate(&self, context: &CompletionContext) -> Result<gio::ListModel, glib::Error> {
            let store = gio::ListStore::new::<super::CitationCompletionProposal>();
            let Some((_begin, prefix)) = context
                .buffer()
                .and_then(|buffer| citation_query_at_cursor(&buffer))
            else {
                return Ok(store.upcast());
            };

            for entry in matching_entries(&self.entries.borrow(), &prefix) {
                let details = format!(
                    "Title: {}\nKey: @{}\nBibliography: {}",
                    entry.title, entry.key, entry.source
                );
                store.append(&super::CitationCompletionProposal::new(
                    &entry.key,
                    &elide(&entry.title, 72),
                    &details,
                ));
            }

            Ok(store.upcast())
        }
    }
}

glib::wrapper! {
    pub struct CitationCompletionProvider(ObjectSubclass<provider_imp::CitationCompletionProvider>)
        @implements sourceview5::CompletionProvider;
}

impl CitationCompletionProvider {
    fn new() -> Self {
        glib::Object::new()
    }

    fn set_entries(&self, entries: Vec<CitationEntry>) {
        self.imp().entries.replace(entries);
    }
}

fn refresh_provider_from_document(
    provider: &CitationCompletionProvider,
    buffer: &sourceview5::Buffer,
    current_file: &Rc<RefCell<Option<PathBuf>>>,
) {
    let markdown = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string();
    let entries = citation_entries(&markdown, current_file.borrow().as_deref());
    provider.set_entries(entries);
}

pub fn setup_citation_completion(
    view: &View,
    buffer: &sourceview5::Buffer,
    current_file: Rc<RefCell<Option<PathBuf>>>,
) {
    let provider = CitationCompletionProvider::new();
    let completion = view.completion();
    completion.set_page_size(8);
    completion.add_provider(&provider);

    refresh_provider_from_document(&provider, buffer, &current_file);

    let provider_for_updates = provider.clone();
    let current_file_for_updates = current_file.clone();
    let reload_debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let reload_debounce_for_signal = reload_debounce.clone();
    buffer.connect_changed(move |buffer| {
        if let Some(source_id) = reload_debounce_for_signal.borrow_mut().take() {
            source_id.remove();
        }

        let provider = provider_for_updates.clone();
        let current_file = current_file_for_updates.clone();
        let reload_debounce = reload_debounce_for_signal.clone();
        let buffer = buffer.clone();
        let source_id =
            glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
                refresh_provider_from_document(&provider, &buffer, &current_file);
                *reload_debounce.borrow_mut() = None;
            });
        *reload_debounce_for_signal.borrow_mut() = Some(source_id);
    });

    let completion_for_insert = completion.clone();
    buffer.connect_insert_text(move |buffer, _iter, text| {
        if text.is_empty() {
            return;
        }

        let last_char = text.chars().last();
        if last_char == Some('@')
            || last_char.map(is_citation_key_char).unwrap_or(false)
                && citation_query_at_cursor(buffer).is_some()
        {
            completion_for_insert.show();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bibliography_metadata() {
        let markdown = r#"---
bibliography:
  - refs/library.bib
  - "refs/other.bib"
---

Body
"#;

        assert_eq!(
            bibliography_paths_from_markdown(markdown),
            vec!["refs/library.bib".to_string(), "refs/other.bib".to_string()]
        );
    }

    #[test]
    fn parses_inline_bibliography_metadata() {
        let markdown = r#"---
bibliography: [refs/a.bib, "refs/b.bib"]
---
"#;

        assert_eq!(
            bibliography_paths_from_markdown(markdown),
            vec!["refs/a.bib".to_string(), "refs/b.bib".to_string()]
        );
    }

    #[test]
    fn extracts_bibtex_entries() {
        let bib = r#"@article{doe2020,
  title = {Example}
}

@book{smith-book,
  title = {A Very Long Book Title}
}

@comment{ignored}
"#;

        assert_eq!(
            bibtex_entries(bib),
            vec![
                ("doe2020".to_string(), Some("Example".to_string())),
                (
                    "smith-book".to_string(),
                    Some("A Very Long Book Title".to_string())
                )
            ]
        );
    }
}
