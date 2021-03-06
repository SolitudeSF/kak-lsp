use crate::context::*;
use crate::types::*;
use crate::util::*;
use itertools::Itertools;
use lsp_types::request::*;
use lsp_types::*;
use regex::Regex;
use serde::Deserialize;
use std;
use url::Url;

pub fn text_document_completion(meta: EditorMeta, params: EditorParams, ctx: &mut Context) {
    let params = TextDocumentCompletionParams::deserialize(params).unwrap();
    let req_params = CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(&meta.buffile).unwrap(),
            },
            position: get_lsp_position(&meta.buffile, &params.position, ctx).unwrap(),
        },
        context: None,
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };
    ctx.call::<Completion, _>(meta, req_params, |ctx: &mut Context, meta, result| {
        editor_completion(meta, params, result, ctx)
    });
}

pub fn editor_completion(
    meta: EditorMeta,
    params: TextDocumentCompletionParams,
    result: Option<CompletionResponse>,
    ctx: &mut Context,
) {
    if result.is_none() {
        return;
    }
    let items = match result.unwrap() {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };
    let unescape_markdown_re = Regex::new(r"\\(?P<c>.)").unwrap();
    let maxlen = items.iter().map(|x| x.label.len()).max().unwrap_or(0);
    let escape_bar = |s: &str| s.replace("|", r"\|");
    let snippet_prefix_re = Regex::new(r"^[^\[\(<\n\$]+").unwrap();

    let items = items
        .into_iter()
        .map(|x| {
            let mut doc: String = match &x.documentation {
                None => "".to_string(),
                Some(doc) => match doc {
                    Documentation::String(st) => st.clone(),
                    Documentation::MarkupContent(mup) => match mup.kind {
                        MarkupKind::PlainText => mup.value.clone(),
                        // NOTE just in case server ignored our documentationFormat capability
                        // we want to unescape markdown to make text a bit more readable
                        MarkupKind::Markdown => unescape_markdown_re
                            .replace_all(&mup.value, r"$c")
                            .to_string(),
                    },
                },
            };
            if let Some(d) = x.detail {
                doc = format!("{}\n\n{}", d, doc);
            }
            let doc = format!("info -style menu {}", editor_quote(&doc));
            let mut entry = x.label.clone();
            if let Some(k) = x.kind {
                entry += &std::iter::repeat(" ")
                    .take(maxlen - x.label.len())
                    .collect::<String>();
                entry += &format!(" {{MenuInfo}}{:?}", k);
            }
            let insert_text = &x.insert_text.unwrap_or(x.label);
            let do_snippet = ctx.config.snippet_support;
            let do_snippet = do_snippet
                && x.insert_text_format
                    .map(|f| f == InsertTextFormat::Snippet)
                    .unwrap_or(false);
            if do_snippet {
                let snippet = insert_text;
                let insert_text = snippet_prefix_re
                    .find(snippet)
                    .map(|x| x.as_str())
                    .unwrap_or(&snippet);
                let command = format!(
                    "{}\nlsp-snippets-insert-completion {} {}",
                    doc,
                    editor_quote(&regex::escape(insert_text)),
                    editor_quote(snippet)
                );
                let command = format!("eval {}", editor_quote(&command));
                editor_quote(&format!(
                    "{}|{}|{}",
                    escape_bar(insert_text),
                    escape_bar(&command),
                    escape_bar(&entry),
                ))
            } else {
                editor_quote(&format!(
                    "{}|{}|{}",
                    escape_bar(insert_text),
                    escape_bar(&doc),
                    escape_bar(&entry),
                ))
            }
        })
        .join(" ");
    let p = params.position;
    let command = format!(
        "set window lsp_completions {}.{}@{} {}\n",
        p.line, params.completion.offset, meta.version, items
    );
    ctx.exec(meta, command);
}
