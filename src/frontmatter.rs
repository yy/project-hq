use std::collections::BTreeMap;

/// Split a markdown document into raw frontmatter text and body slices.
pub fn split_frontmatter(text: &str) -> Result<(&str, &str), &'static str> {
    if !text.starts_with("---") {
        return Err("No frontmatter");
    }

    let rest = &text[3..];
    let end = rest
        .match_indices("---")
        .find(|(i, _)| *i == 0 || rest.as_bytes().get(i - 1) == Some(&b'\n'))
        .map(|(i, _)| i)
        .ok_or("Malformed frontmatter")?;

    Ok((&rest[..end], &rest[end + 3..]))
}

/// Parse simple `key: value` fields from frontmatter.
pub fn parse_frontmatter(text: &str) -> Option<BTreeMap<String, String>> {
    let (fm_text, _) = split_frontmatter(text).ok()?;

    let mut fields = BTreeMap::new();
    for line in fm_text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().trim_matches('"').to_string();
            if !value.is_empty() {
                fields.insert(key, value);
            }
        }
    }

    if fields.contains_key("title") && fields.contains_key("status") {
        Some(fields)
    } else {
        None
    }
}
