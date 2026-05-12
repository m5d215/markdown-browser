use std::time::Duration;

/// Fetch the body of `url` over HTTP(S). Used for both the initial CLI
/// invocation (`markdown-browser https://...`) and in-app navigation of
/// markdown links pointing at remote files.
pub fn fetch(url: &str) -> Result<String, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(url)
        .header("Accept", "text/markdown, text/plain;q=0.9, text/*;q=0.5")
        .call()
        .map_err(|e| e.to_string())?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}
