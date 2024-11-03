use anyhow::bail;

#[cfg(not(target_arch = "wasm32"))]
fn send_reqwest(url: &str) -> anyhow::Result<reqwest::blocking::Response> {
    let client = reqwest::blocking::Client::new();
    let response = client.get(url).send()?;
    if !response.status().is_success() {
        bail!("Failed to download: {}", response.status());
    }
    Ok(response)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn download_text(url: &str) -> anyhow::Result<String> {
    let response = send_reqwest(url)?;
    Ok(response.text()?)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn download_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    let response = send_reqwest(url)?;
    Ok(response.bytes()?.to_vec())
}

#[cfg(target_arch = "wasm32")]
fn send_xhr(url: &str, response_type: web_sys::XmlHttpRequestResponseType) -> anyhow::Result<web_sys::XmlHttpRequest> {
    let xhr = web_sys::XmlHttpRequest::new().unwrap();
    xhr.open_with_async("GET", url, false).map_err(|e| anyhow::anyhow!(e))?;
    xhr.set_response_type(response_type);
    xhr.send().map_err(|e| anyhow::anyhow!(e))?;
    if (xhr.status() / 100) != 2 {
        bail!("Failed to download: {}", xhr.status());
    }
    Ok(xhr)
}

#[cfg(target_arch = "wasm32")]
pub fn download_text(url: &str) -> anyhow::Result<String> {
    let xhr = send_xhr(url, web_sys::XmlHttpRequestResponseType::Text)?;
    Ok(xhr.response_text().unwrap().ok_or("Failed to get response text")?)
}

#[cfg(target_arch = "wasm32")]
pub fn download_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    let xhr = send_xhr(url, web_sys::XmlHttpRequestResponseType::Arraybuffer)?;
    let buf = js_sys::Uint8Array::new(&xhr.response()).to_vec();
    Ok(buf)
}


