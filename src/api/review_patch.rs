//! Select changed lines from a bounded, full JGit unified patch; never execute it.
use crate::{
    error::{GbMcpError, Result},
    models::review_post::CreateReviewComment,
};
use serde_json::{json, Value};

fn unsupported() -> GbMcpError {
    GbMcpError::Unsupported(
        "No unambiguous supported textual PR diff could verify this path/line; no write was sent"
            .into(),
    )
}

fn git_path(header: &str, prefix: &str) -> Result<Option<String>> {
    let raw = if header.starts_with('"') {
        let mut bytes = Vec::new();
        let mut iter = header.as_bytes()[1..].iter().copied();
        let mut closed = false;
        while let Some(byte) = iter.next() {
            if byte == b'"' {
                closed = true;
                break;
            }
            if byte != b'\\' {
                bytes.push(byte);
                continue;
            }
            let next = iter.next().ok_or_else(unsupported)?;
            bytes.push(match next {
                b'\\' | b'"' => next,
                b'a' => 7,
                b'b' => 8,
                b't' => 9,
                b'n' => 10,
                b'v' => 11,
                b'f' => 12,
                b'r' => 13,
                b'0'..=b'3' => {
                    let b = iter
                        .next()
                        .filter(|c| (b'0'..=b'7').contains(c))
                        .ok_or_else(unsupported)?;
                    let c = iter
                        .next()
                        .filter(|c| (b'0'..=b'7').contains(c))
                        .ok_or_else(unsupported)?;
                    (next - b'0') * 64 + (b - b'0') * 8 + (c - b'0')
                }
                _ => return Err(unsupported()),
            });
        }
        if !closed || iter.any(|c| c != b'\t' && c != b' ') {
            return Err(unsupported());
        }
        String::from_utf8(bytes).map_err(|_| unsupported())?
    } else {
        header.trim_end_matches('\t').to_owned()
    };
    if raw == "/dev/null" {
        return Ok(None);
    }
    raw.strip_prefix(prefix)
        .map(|v| Some(v.to_owned()))
        .ok_or_else(unsupported)
}

fn range(value: &str) -> Result<(u64, u64)> {
    let (start, count) = value.split_once(',').unwrap_or((value, "1"));
    let parse = |v: &str| {
        if !v.is_empty() && v.bytes().all(|c| c.is_ascii_digit()) {
            v.parse::<u64>().ok()
        } else {
            None
        }
    };
    let (start, count) = (
        parse(start).ok_or_else(unsupported)?,
        parse(count).ok_or_else(unsupported)?,
    );
    start.checked_add(count).ok_or_else(unsupported)?;
    if start == 0 && count != 0 {
        return Err(unsupported());
    }
    Ok((start, count))
}

pub(super) fn select(patch: &str, request: &CreateReviewComment) -> Result<Value> {
    let lines: Vec<_> = patch
        .split_terminator('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let boundaries: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| line.starts_with("diff --git ").then_some(i))
        .chain(std::iter::once(lines.len()))
        .collect();
    let mut selected = None;
    for pair in boundaries.windows(2) {
        let block = &lines[pair[0] + 1..pair[1]];
        let header_end = block
            .iter()
            .position(|line| line.starts_with("@@ "))
            .unwrap_or(block.len());
        let old: Vec<_> = block[..header_end]
            .iter()
            .filter_map(|line| line.strip_prefix("--- "))
            .collect();
        let new: Vec<_> = block[..header_end]
            .iter()
            .filter_map(|line| line.strip_prefix("+++ "))
            .collect();
        if old.len() != 1 || new.len() != 1 {
            continue;
        }
        let (old, new) = (git_path(old[0], "a/")?, git_path(new[0], "b/")?);
        if new.as_deref() != Some(&request.path) && old.as_deref() != Some(&request.path) {
            continue;
        }
        if selected.is_some()
            || new.is_none()
            || new.as_deref() != Some(&request.path)
            || (old.is_some() && old != new)
        {
            return Err(unsupported());
        }
        let metadata = &block[..header_end];
        if metadata.iter().any(|line| {
            line.starts_with("rename ")
                || line.starts_with("copy ")
                || line.starts_with("similarity ")
                || line.starts_with("Binary ")
                || line.starts_with("GIT binary")
        }) {
            return Err(unsupported());
        }
        let modes: Vec<_> = metadata
            .iter()
            .filter_map(|line| {
                line.strip_prefix("new file mode ")
                    .or_else(|| line.strip_prefix("old mode "))
                    .or_else(|| line.strip_prefix("new mode "))
                    .or_else(|| {
                        line.strip_prefix("index ")
                            .and_then(|v| v.split_once(' ').map(|(_, mode)| mode))
                    })
            })
            .collect();
        if modes.is_empty()
            || modes
                .iter()
                .any(|mode| !matches!(*mode, "100644" | "100755"))
        {
            return Err(unsupported());
        }
        let (mut i, mut old_end, mut new_end) = (header_end, 0, 0);
        let mut found = None;
        while i < block.len() {
            let header = block[i]
                .strip_prefix("@@ -")
                .and_then(|v| v.split_once(" @@"))
                .map(|(v, _)| v)
                .ok_or_else(unsupported)?;
            let (a, b) = header.split_once(" +").ok_or_else(unsupported)?;
            let ((mut old_line, mut old_left), (mut new_line, mut new_left)) =
                (range(a)?, range(b)?);
            if old_line < old_end || new_line < new_end {
                return Err(unsupported());
            }
            old_end = old_line.checked_add(old_left).ok_or_else(unsupported)?;
            new_end = new_line.checked_add(new_left).ok_or_else(unsupported)?;
            i += 1;
            while old_left != 0 || new_left != 0 {
                let line = block.get(i).ok_or_else(unsupported)?;
                if *line == "\\ No newline at end of file" {
                    i += 1;
                    continue;
                }
                let (prefix, text) = line
                    .as_bytes()
                    .first()
                    .copied()
                    .zip(line.get(1..))
                    .ok_or_else(unsupported)?;
                match prefix {
                    b' ' if old_left > 0 && new_left > 0 => {
                        old_left -= 1;
                        new_left -= 1;
                        old_line += 1;
                        new_line += 1;
                    }
                    b'-' if old_left > 0 => {
                        if request.old_line == Some(old_line) {
                            found = Some(
                                json!([{"oldLine":old_line.to_string(),"type":"delete","text":text}]),
                            );
                        }
                        old_left -= 1;
                        old_line += 1;
                    }
                    b'+' if new_left > 0 => {
                        if request.new_line == Some(new_line) {
                            found = Some(
                                json!([{"newLine":new_line.to_string(),"type":"insert","text":text}]),
                            );
                        }
                        new_left -= 1;
                        new_line += 1;
                    }
                    _ => return Err(unsupported()),
                }
                i += 1;
            }
            if block.get(i) == Some(&"\\ No newline at end of file") {
                i += 1;
            }
        }
        selected = Some(found.ok_or_else(|| {
            GbMcpError::InvalidInput(
                "The selected side/line is not a changed line in the current PR diff".into(),
            )
        })?);
    }
    selected.ok_or_else(unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::review_post::HeadGuard;
    fn change() -> CreateReviewComment {
        CreateReviewComment {
            body: "review".into(),
            path: "review.txt".into(),
            commit: "b".repeat(40),
            old_line: None,
            new_line: Some(2),
            head_guard: HeadGuard::Preflight,
        }
    }
    const PATCH: &str = "diff --git a/review.txt b/review.txt\nindex 123..456 100644\n--- a/review.txt\n+++ b/review.txt\n@@ -1,3 +1,3 @@\n alpha\n-old\n+new\n omega\n";
    #[test]
    fn changed_sides_are_distinct_and_context_or_absent_lines_are_rejected() {
        let mut r = change();
        assert_eq!(select(PATCH, &r).unwrap()[0]["newLine"], "2");
        r.new_line = None;
        r.old_line = Some(2);
        assert_eq!(select(PATCH, &r).unwrap()[0]["type"], "delete");
        r.old_line = Some(1);
        assert!(select(PATCH, &r).is_err());
        r.old_line = Some(99);
        assert!(select(PATCH, &r).is_err());
    }
    #[test]
    fn quoted_utf8_paths_spaces_quotes_and_backslashes_are_decoded_exactly() {
        assert_eq!(
            git_path(r#""a/\346\227\245\346\234\254 & \"x\"\\file.txt""#, "a/")
                .unwrap()
                .unwrap(),
            "日本 & \"x\"\\file.txt"
        );
        let mut r = change();
        r.path = "file with space.txt".into();
        let patch = PATCH.replace("review.txt", "file with space.txt");
        assert!(select(&patch, &r).is_ok());
        r.path = "file with space".into();
        assert!(select(&patch, &r).is_err());
    }
    #[test]
    fn multiple_hunks_and_no_newline_markers_keep_absolute_line_numbers() {
        let patch="diff --git a/review.txt b/review.txt\nindex 123..456 100644\n--- a/review.txt\n+++ b/review.txt\n@@ -1 +1 @@\n-old\n+new\n@@ -9 +9 @@\n-last\n\\ No newline at end of file\n+next\n\\ No newline at end of file\n";
        let mut r = change();
        r.new_line = Some(9);
        assert_eq!(select(patch, &r).unwrap()[0]["text"], "next");
    }
    #[test]
    fn ambiguous_binary_modes_renames_deletions_and_bad_counts_fail_closed() {
        for patch in [
            PATCH.replace("100644", "120000"),
            PATCH.replace("+++ b/review.txt", "+++ /dev/null"),
            PATCH.replace("--- a/review.txt", "--- a/old.txt"),
            PATCH.replace("-1,3 +1,3", "-1,4 +1,3"),
            format!("{PATCH}{PATCH}"),
            "diff --git a/review.txt b/review.txt\nBinary files differ\n".into(),
        ] {
            assert!(select(&patch, &change()).is_err());
        }
        let added="diff --git a/review.txt b/review.txt\nnew file mode 100644\nindex 000..456\n--- /dev/null\n+++ b/review.txt\n@@ -0,0 +1,2 @@\n+one\n+two\n";
        assert!(select(added, &change()).is_ok());
    }
}
