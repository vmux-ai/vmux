pub fn extension_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if is_ext_id(trimmed) {
        return Some(trimmed.to_string());
    }
    trimmed
        .split(['/', '?', '#'])
        .find(|seg| is_ext_id(seg))
        .map(|s| s.to_string())
}

fn is_ext_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

pub fn crx_url(id: &str, prodversion: &str) -> String {
    format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&acceptformat=crx2,crx3&prodversion={prodversion}&x=id%3D{id}%26installsource%3Dondemand%26uc"
    )
}

#[cfg(test)]
#[path = "webstore.test.rs"]
mod tests;
