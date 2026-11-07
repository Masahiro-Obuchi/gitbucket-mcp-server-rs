//! Deliberate list projections; detail getters keep the original models.
use serde_json::Value;

use super::list_page::ListKind;

fn retain(value: &mut Value, names: &[&str], path: &str, omitted: &mut Vec<String>) {
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, _| {
            let keep = names.contains(&key.as_str());
            if !keep {
                omitted.push(format!("{path}/{key}"));
            }
            keep
        });
    }
}

fn compact(value: &mut Value, path: &str, omitted: &mut Vec<String>, truncated: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let child_path = format!("{path}/{key}");
                match key.as_str() {
                    "user" | "owner" | "creator" | "assignee" | "merged_by" => {
                        retain(child, &["login"], &child_path, omitted);
                    }
                    "head" | "base" => {
                        retain(
                            child,
                            &["label", "ref", "sha", "repo", "user"],
                            &child_path,
                            omitted,
                        );
                        if let Some(repo) = child.get_mut("repo") {
                            retain(repo, &["full_name"], &format!("{child_path}/repo"), omitted);
                        }
                    }
                    _ => {}
                }
                // Preserve callable identifiers and URLs exactly. An oversized
                // identity causes a bounded-response error, never a wrong target.
                let identity = matches!(
                    key.as_str(),
                    "id" | "name"
                        | "full_name"
                        | "login"
                        | "sha"
                        | "ref"
                        | "path"
                        | "url"
                        | "author"
                        | "default_branch"
                ) || key.ends_with("_url");
                if let Value::String(text) = child {
                    if !identity && text.len() > 1024 {
                        let mut end = 1024;
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        text.truncate(end);
                        truncated.push(child_path.clone());
                    }
                }
                compact(child, &child_path, omitted, truncated);
            }
        }
        Value::Array(items) => {
            if items.len() > 20 {
                items.truncate(20);
                truncated.push(path.to_owned());
            }
            for (index, item) in items.iter_mut().enumerate() {
                compact(item, &format!("{path}/{index}"), omitted, truncated);
            }
        }
        _ => {}
    }
}

pub(super) fn project(kind: ListKind, mut value: Value) -> Value {
    let mut omitted = Vec::new();
    let mut truncated = Vec::new();
    let keys: &[&str] = match kind {
        ListKind::Repositories => &[
            "name",
            "full_name",
            "description",
            "html_url",
            "private",
            "fork",
            "default_branch",
            "owner",
            "open_issues_count",
        ],
        ListKind::Branches => &["name", "commit"],
        ListKind::Labels => &["name", "color", "description", "url"],
        ListKind::Milestones => &[
            "number",
            "title",
            "state",
            "due_on",
            "html_url",
            "open_issues",
            "closed_issues",
        ],
        ListKind::Issues => &[
            "number",
            "title",
            "state",
            "html_url",
            "created_at",
            "updated_at",
            "closed_at",
            "comments",
            "user",
            "labels",
        ],
        ListKind::PullRequests => &[
            "number",
            "title",
            "state",
            "draft",
            "merged",
            "mergeable",
            "html_url",
            "created_at",
            "updated_at",
            "head",
            "base",
            "user",
        ],
        ListKind::Comments => &["id", "body", "user", "created_at", "updated_at", "html_url"],
        ListKind::ReviewComments => &[
            "id",
            "storage_id",
            "kind",
            "author",
            "path",
            "observed_old_line",
            "observed_new_line",
            "current_commit",
            "position_verified",
            "association",
            "source",
            "url",
            "created_at_display",
        ],
    };
    retain(&mut value, keys, "", &mut omitted);
    compact(&mut value, "", &mut omitted, &mut truncated);
    value
        .as_object_mut()
        .expect("list item model is an object")
        .insert(
            "_list".into(),
            serde_json::json!({"omitted_fields":omitted,"truncated_fields":truncated}),
        );
    value
}
