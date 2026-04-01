use std::collections::BTreeMap;

/// Split a markdown document into raw frontmatter text and body slices.
pub fn split_frontmatter(text: &str) -> Result<(&str, &str), &'static str> {
    if !text.starts_with("---") {
        return Err("No frontmatter");
    }

    let rest = &text[3..];
    if !(rest.starts_with('\n') || rest.starts_with("\r\n")) {
        return Err("Malformed frontmatter");
    }

    let mut offset = 0;
    while offset < rest.len() {
        let line_end = rest[offset..]
            .find('\n')
            .map(|i| offset + i)
            .unwrap_or(rest.len());
        let line = rest[offset..line_end].trim_end_matches('\r');

        if line == "---" {
            return Ok((&rest[..offset], &rest[line_end..]));
        }

        if line_end == rest.len() {
            break;
        }
        offset = line_end + 1;
    }

    Err("Malformed frontmatter")
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
