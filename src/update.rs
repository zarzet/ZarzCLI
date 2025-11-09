use anyhow::Result;
use serde::Deserialize;

const CURRENT_VERSION: &str = "0.5.0-alpha";
const PACKAGE_NAME: &str = "zarz";
const NPM_REGISTRY_URL: &str = "https://registry.npmjs.org";

#[derive(Debug, Deserialize)]
struct NpmPackageInfo {
    #[serde(rename = "dist-tags")]
    dist_tags: DistTags,
}

#[derive(Debug, Deserialize)]
struct DistTags {
    latest: String,
}

pub async fn check_for_updates() -> Result<Option<String>> {
    let url = format!("{}/{}", NPM_REGISTRY_URL, PACKAGE_NAME);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let package_info: NpmPackageInfo = response.json().await?;
    let latest_version = package_info.dist_tags.latest;

    if latest_version != CURRENT_VERSION && !latest_version.is_empty() {
        Ok(Some(latest_version))
    } else {
        Ok(None)
    }
}

pub fn print_update_notification(new_version: &str) {
    println!("\n╭─────────────────────────────────────────────────────────╮");
    println!("│  Update Available!                                      │");
    println!("│                                                         │");
    println!("│  Current version: {}                            │", CURRENT_VERSION);
    println!("│  Latest version:  {}                            │", new_version);
    println!("│                                                         │");
    println!("│  Run: npm update -g zarz                                │");
    println!("╰─────────────────────────────────────────────────────────╯\n");
}
