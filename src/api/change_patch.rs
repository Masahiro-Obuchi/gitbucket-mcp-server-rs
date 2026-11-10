//! Complete bounded JGit patch parsing; never infer renames from add/delete pairs.
use super::review_patch::{git_path, range};
use crate::{
    error::{GbMcpError, Result},
    models::pull_request_changes::{FileChangeStatus, PullRequestFile},
};
use std::collections::HashSet;
fn bad() -> GbMcpError {
    GbMcpError::Unsupported("GitBucket patch has unsupported, ambiguous or incomplete structure; no partial file/diff result was returned".into())
}
fn one<'a>(lines: &[&'a str], prefix: &str) -> Result<Option<&'a str>> {
    let mut found = lines.iter().filter_map(|l| l.strip_prefix(prefix));
    let value = found.next();
    if found.next().is_some() {
        return Err(bad());
    }
    Ok(value)
}
fn mode(value: Option<&str>) -> Result<Option<String>> {
    value
        .map(|v| {
            if v.len() == 6 && v.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
                Ok(v.to_owned())
            } else {
                Err(bad())
            }
        })
        .transpose()
}
struct PatchIndex<'a> {
    old: &'a str,
    new: &'a str,
    mode: Option<String>,
}
fn index(value: Option<&str>) -> Result<Option<PatchIndex<'_>>> {
    value
        .map(|value| {
            let (ids, file_mode) = value
                .split_once(' ')
                .map_or((value, None), |(ids, file_mode)| (ids, Some(file_mode)));
            let (old, new) = ids.split_once("..").ok_or_else(bad)?;
            if [old, new].iter().any(|id| {
                id.is_empty() || id.len() > 40 || !id.bytes().all(|b| b.is_ascii_hexdigit())
            }) {
                return Err(bad());
            }
            Ok(PatchIndex {
                old,
                new,
                mode: mode(file_mode)?,
            })
        })
        .transpose()
}
fn empty_blob(id: &str) -> bool {
    // Native headers abbreviate object IDs, including the SHA-1 of an empty blob.
    id.len() >= 7 && "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391".starts_with(id)
}
fn similarity(value: Option<&str>) -> Result<Option<u8>> {
    value
        .map(|v| {
            v.strip_suffix('%')
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|s| s.parse::<u8>().ok())
                .filter(|score| *score <= 100)
                .ok_or_else(bad)
        })
        .transpose()
}
fn header_paths(header: &str) -> Result<Vec<(String, String)>> {
    if header.len() > 16 * 1024 {
        return Err(bad());
    }
    let mut pairs = Vec::new();
    for (i, _) in header.match_indices(' ') {
        let right = &header[i + 1..];
        if !right.starts_with("b/") && !right.starts_with("\"b/") {
            continue;
        }
        if let (Ok(Some(a)), Ok(Some(b))) = (
            git_path(&header[..i], "a/"),
            git_path(&header[i + 1..], "b/"),
        ) {
            pairs.push((a, b));
            if pairs.len() > 32 {
                return Err(bad());
            }
        }
    }
    Ok(pairs)
}
fn hunks(lines: &[&str]) -> Result<(usize, u64, u64)> {
    let (mut i, mut count, mut add, mut del) = (0, 0, 0u64, 0u64);
    let (mut old_end, mut new_end) = (0, 0);
    while i < lines.len() {
        let header = lines[i]
            .strip_prefix("@@ -")
            .and_then(|v| v.split_once(" @@"))
            .map(|(v, _)| v)
            .ok_or_else(bad)?;
        let (a, b) = header.split_once(" +").ok_or_else(bad)?;
        let ((old, mut a), (new, mut b)) =
            (range(a).map_err(|_| bad())?, range(b).map_err(|_| bad())?);
        if old < old_end || new < new_end {
            return Err(bad());
        }
        old_end = old.checked_add(a).ok_or_else(bad)?;
        new_end = new.checked_add(b).ok_or_else(bad)?;
        count += 1;
        i += 1;
        while a != 0 || b != 0 {
            let line = *lines.get(i).ok_or_else(bad)?;
            match line.as_bytes().first() {
                Some(b' ') if a > 0 && b > 0 => {
                    a -= 1;
                    b -= 1;
                }
                Some(b'-') if a > 0 => {
                    a -= 1;
                    del += 1;
                }
                Some(b'+') if b > 0 => {
                    b -= 1;
                    add += 1;
                }
                Some(b'\\') if line == "\\ No newline at end of file" => {}
                _ => return Err(bad()),
            }
            i += 1;
        }
        if lines.get(i) == Some(&"\\ No newline at end of file") {
            i += 1;
        }
    }
    Ok((count, add, del))
}
pub(super) struct FileBlock<'a> {
    pub file: PullRequestFile,
    pub patch: &'a str,
}
pub(super) fn parse(patch: &str) -> Result<Vec<FileBlock<'_>>> {
    if patch.is_empty() {
        return Ok(vec![]);
    }
    // Even a source file without a final LF has a terminated patch line followed
    // by the no-newline marker. An unterminated patch line is a truncated source.
    if !patch.ends_with('\n') {
        return Err(bad());
    }
    let (mut starts, mut offset) = (Vec::new(), 0);
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            starts.push(offset);
        }
        offset += line.len();
    }
    if starts.first() != Some(&0) {
        return Err(bad());
    }
    starts.push(patch.len());
    let (mut result, mut seen) = (Vec::new(), HashSet::new());
    for bounds in starts.windows(2) {
        let raw = &patch[bounds[0]..bounds[1]];
        let lines: Vec<_> = raw.split_terminator('\n').collect();
        let header = lines[0].strip_prefix("diff --git ").ok_or_else(bad)?;
        let first = lines
            .iter()
            .position(|l| l.starts_with("@@ "))
            .unwrap_or(lines.len());
        let meta = &lines[1..first];
        if meta.is_empty() {
            return Err(bad());
        }
        let rename = (one(meta, "rename from ")?, one(meta, "rename to ")?);
        let copy = (one(meta, "copy from ")?, one(meta, "copy to ")?);
        if rename.0.is_some() != rename.1.is_some()
            || copy.0.is_some() != copy.1.is_some()
            || (rename.0.is_some() && copy.0.is_some())
        {
            return Err(bad());
        }
        let similarity = similarity(one(meta, "similarity index ")?)?;
        if similarity.is_some() != (rename.0.is_some() || copy.0.is_some()) {
            return Err(bad());
        }
        let added = one(meta, "new file mode ")?;
        let deleted = one(meta, "deleted file mode ")?;
        if added.is_some() && deleted.is_some() {
            return Err(bad());
        }
        let old_header = one(meta, "--- ")?;
        let new_header = one(meta, "+++ ")?;
        if old_header.is_some() != new_header.is_some() {
            return Err(bad());
        }
        let extended = if let (Some(a), Some(b)) = (rename.0.or(copy.0), rename.1.or(copy.1)) {
            Some((
                git_path(a, "").map_err(|_| bad())?,
                git_path(b, "").map_err(|_| bad())?,
            ))
        } else {
            None
        };
        let (old, new) = if let (Some(a), Some(b)) = (old_header, new_header) {
            (
                git_path(a, "a/").map_err(|_| bad())?,
                git_path(b, "b/").map_err(|_| bad())?,
            )
        } else if let Some(p) = extended.clone() {
            p
        } else {
            let mut pairs = header_paths(header)?.into_iter().filter(|(a, b)| a == b);
            let p = pairs.next().ok_or_else(bad)?;
            if pairs.next().is_some() {
                return Err(bad());
            }
            (
                if added.is_some() { None } else { Some(p.0) },
                if deleted.is_some() { None } else { Some(p.1) },
            )
        };
        if (old.is_none() && new.is_none())
            || added.is_some() != old.is_none()
            || deleted.is_some() != new.is_none()
        {
            return Err(bad());
        }
        if extended
            .as_ref()
            .is_some_and(|p| p != &(old.clone(), new.clone()))
        {
            return Err(bad());
        }
        let expected = (
            old.as_ref().or(new.as_ref()).ok_or_else(bad)?,
            new.as_ref().or(old.as_ref()).ok_or_else(bad)?,
        );
        if !header_paths(header)?
            .iter()
            .any(|(a, b)| a == expected.0 && b == expected.1)
        {
            return Err(bad());
        }
        for p in [old.as_deref(), new.as_deref()].into_iter().flatten() {
            if p.is_empty() || p.contains('\0') {
                return Err(bad());
            }
        }
        if !seen.insert((old.clone(), new.clone())) {
            return Err(bad());
        }
        let index = index(one(meta, "index ")?)?;
        let index_mode = index.as_ref().and_then(|i| i.mode.as_deref());
        let old_mode_header = one(meta, "old mode ")?;
        let new_mode_header = one(meta, "new mode ")?;
        if old_mode_header.is_some() != new_mode_header.is_some()
            || ((added.is_some() || deleted.is_some()) && old_mode_header.is_some())
            || index_mode.is_some_and(|m| {
                [added, deleted, old_mode_header, new_mode_header]
                    .into_iter()
                    .flatten()
                    .any(|header| header != m)
            })
        {
            return Err(bad());
        }
        if index.as_ref().is_some_and(|i| {
            old.is_none() != i.old.bytes().all(|b| b == b'0')
                || new.is_none() != i.new.bytes().all(|b| b == b'0')
        }) || (index.is_none() && (added.is_some() || deleted.is_some()))
        {
            return Err(bad());
        }
        let old_mode = if old.is_none() {
            None
        } else {
            mode(deleted.or(old_mode_header).or(index_mode))?
        };
        let new_mode = if new.is_none() {
            None
        } else {
            mode(added.or(new_mode_header).or(index_mode))?
        };
        let binary = meta
            .iter()
            .any(|l| l.starts_with("Binary files ") && l.ends_with(" differ"));
        if meta
            .iter()
            .any(|l| l.starts_with("Binary files ") && !l.ends_with(" differ"))
        {
            return Err(bad());
        }
        if meta.iter().any(|l| {
            ![
                "index ",
                "new file mode ",
                "deleted file mode ",
                "old mode ",
                "new mode ",
                "similarity index ",
                "dissimilarity index ",
                "rename from ",
                "rename to ",
                "copy from ",
                "copy to ",
                "--- ",
                "+++ ",
                "Binary files ",
            ]
            .iter()
            .any(|p| l.starts_with(p))
        }) {
            return Err(bad());
        }
        if binary && first != lines.len() {
            return Err(bad());
        }
        let (count, add, del) = hunks(&lines[first..])?;
        if count > 0
            && (old_header.is_none()
                || index.is_none()
                || (add == 0 && del == 0)
                || (added.is_some() && del != 0)
                || (deleted.is_some() && add != 0))
        {
            return Err(bad());
        }
        // Mode/rename metadata does not make missing content complete. Changed
        // object IDs need hunks or a binary marker; only empty adds/deletes can
        // legitimately have different IDs and no content lines.
        if count == 0
            && !binary
            && (similarity.is_some_and(|score| score < 100)
                || index.as_ref().is_some_and(|i| {
                    i.old != i.new
                        && !(added.is_some() && empty_blob(i.new)
                            || deleted.is_some() && empty_blob(i.old))
                }))
        {
            return Err(bad());
        }
        if count == 0
            && !binary
            && added.is_none()
            && deleted.is_none()
            && rename.0.is_none()
            && copy.0.is_none()
            && (old_mode.is_none() || old_mode == new_mode)
        {
            return Err(bad());
        }
        let status = if old.is_none() {
            FileChangeStatus::Added
        } else if new.is_none() {
            FileChangeStatus::Deleted
        } else if rename.0.is_some() {
            FileChangeStatus::Renamed
        } else if copy.0.is_some() {
            FileChangeStatus::Copied
        } else {
            if old != new {
                return Err(bad());
            }
            FileChangeStatus::Modified
        };
        result.push(FileBlock {
            file: PullRequestFile {
                old_path: old,
                new_path: new,
                status,
                old_mode,
                new_mode,
                binary,
                hunks: count,
                additions: (!binary).then_some(add),
                deletions: (!binary).then_some(del),
                patch_bytes: raw.len(),
            },
            patch: raw,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEXT: &str="diff --git a/file.txt b/file.txt\nindex 123..456 100644\n--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@\n---old content\n+++new content\n context\n\\ No newline at end of file\n";
    #[test]
    fn exact_text_crlf_no_newline_markers_and_hunk_counts_are_preserved() {
        for patch in [TEXT.to_owned(), TEXT.replace("content\n", "content\r\n")] {
            let blocks = parse(&patch).unwrap();
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].patch, patch);
            assert_eq!(blocks[0].file.additions, Some(1));
            assert_eq!(blocks[0].file.deletions, Some(1));
            assert_eq!(blocks[0].file.hunks, 1);
        }
    }
    #[test]
    fn empty_added_and_deleted_files_modes_symlinks_and_gitlinks_are_explicit() {
        for (label, mode) in [("new file mode", "100644"), ("deleted file mode", "100755")] {
            let ids = if label == "new file mode" {
                "000..e69de29"
            } else {
                "e69de29..000"
            };
            let patch = format!("diff --git a/empty b/empty\n{label} {mode}\nindex {ids}\n");
            let b = parse(&patch).unwrap();
            assert_eq!(b[0].file.hunks, 0);
            assert_eq!(b[0].file.old_path.is_none(), label == "new file mode");
            assert_eq!(b[0].file.new_path.is_none(), label == "deleted file mode");
        }
        for mode in ["120000", "160000"] {
            let p = TEXT.replace("100644", mode);
            assert_eq!(parse(&p).unwrap()[0].file.new_mode.as_deref(), Some(mode));
        }
    }
    #[test]
    fn index_mode_and_absent_side_metadata_must_be_consistent() {
        for patch in [
            TEXT.replace("123..456", "not-an-index"),
            TEXT.replace("123..456", "123..xyz"),
            TEXT.replace("123..456", "000..456"),
            TEXT.replace("index 123..456 100644", "old mode 100644"),
            TEXT.replace(
                "index 123..456 100644",
                "old mode 100644\nnew mode 100755\nindex 123..456 100644",
            ),
            "diff --git a/empty b/empty\nnew file mode 100644\n".into(),
            "diff --git a/empty b/empty\ndeleted file mode 100644\nindex 0000000..e69de29\n".into(),
            "diff --git a/old b/new\nsimilarity index broken\nrename from old\nrename to new\n"
                .into(),
        ] {
            assert!(parse(&patch).is_err(), "{patch}");
        }
        let mode_and_content = TEXT.replace(
            "index 123..456 100644",
            "old mode 100644\nnew mode 100755\nindex 123..456",
        );
        let blocks = parse(&mode_and_content).unwrap();
        assert_eq!(blocks[0].file.old_mode.as_deref(), Some("100644"));
        assert_eq!(blocks[0].file.new_mode.as_deref(), Some("100755"));
        assert_eq!(blocks[0].file.additions, Some(1));
    }
    #[test]
    fn quoted_paths_copy_rename_and_unquoted_ambiguous_spaces_keep_exact_identity() {
        let unicode = TEXT
            .replace("a/file.txt", r#""a/\346\227\245\346\234\254.txt""#)
            .replace("b/file.txt", r#""b/\346\227\245\346\234\254.txt""#);
        assert_eq!(
            parse(&unicode).unwrap()[0].file.new_path.as_deref(),
            Some("日本.txt")
        );
        let spaces = TEXT.replace("file.txt", "name b/more spaces.txt");
        assert_eq!(
            parse(&spaces).unwrap()[0].file.new_path.as_deref(),
            Some("name b/more spaces.txt")
        );
        let copy="diff --git a/old name b/new name\nsimilarity index 100%\ncopy from old name\ncopy to new name\n";
        assert_eq!(
            parse(copy).unwrap()[0].file.status,
            FileChangeStatus::Copied
        );
        assert_eq!(
            parse(&copy.replace("copy", "rename")).unwrap()[0]
                .file
                .status,
            FileChangeStatus::Renamed
        );
    }
    #[test]
    fn truncated_hunks_duplicate_files_header_mismatches_and_unknown_formats_fail_closed() {
        for p in [
            TEXT.replace("-1,2 +1,2", "-1,3 +1,2"),
            TEXT.replace("+++ b/file.txt", "+++ b/wrong.txt"),
            TEXT.replace("100644", "broken"),
            TEXT.replace("index 123..456 100644", "unrecognized metadata"),
            format!("{TEXT}{TEXT}"),
            "diff --cc file\n".into(),
            "diff --git a/x b/x\nGIT binary patch\nliteral 5\n".into(),
            "diff --git a/x b/x\nBinary files truncated".into(),
        ] {
            assert!(parse(&p).is_err(), "{p}");
        }
        assert!(parse("").unwrap().is_empty());
    }
}
