//! Explicit user attachments stay ephemeral and never enter profile/history JSON.
use crate::{Error, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Cursor;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub name: String,
    pub data: String,
}
#[derive(Serialize)]
pub struct Support {
    pub images: bool,
    pub known: bool,
}
fn invalid() -> Error {
    Error::Validation(
        "Choisis au maximum quatre images PNG, JPEG ou WebP de moins de 10 Mio.".into(),
    )
}
pub fn normalize(inputs: Vec<Input>) -> Result<Vec<String>> {
    if inputs.len() > 4 {
        return Err(invalid());
    }
    let mut out = vec![];
    for input in inputs {
        if input.name.len() > 240
            || input.name.chars().any(char::is_control)
            || input.data.len() > 14 * 1024 * 1024
        {
            return Err(invalid());
        }
        let data = [
            "data:image/png;base64,",
            "data:image/jpeg;base64,",
            "data:image/webp;base64,",
        ]
        .iter()
        .find_map(|p| input.data.strip_prefix(p))
        .ok_or_else(invalid)?;
        let bytes = STANDARD.decode(data).map_err(|_| invalid())?;
        if bytes.len() > 10 * 1024 * 1024 {
            return Err(invalid());
        }
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| invalid())?;
        if !matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
        ) {
            return Err(invalid());
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(96 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|_| invalid())?
            .thumbnail(1024, 1024);
        let mut png = Cursor::new(vec![]);
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|_| invalid())?;
        out.push(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(png.into_inner())
        ));
    }
    Ok(out)
}
pub fn inject(body: &mut Value, images: &[String], anthropic: bool) -> Result<()> {
    if images.is_empty() {
        return Ok(());
    }
    let message = body["messages"]
        .as_array_mut()
        .and_then(|m| m.iter_mut().rev().find(|m| m["role"] == "user"))
        .ok_or_else(invalid)?;
    let text = message["content"].as_str().ok_or_else(invalid)?.to_owned();
    let mut content = vec![json!({"type":"text","text":text})];
    for data in images {
        content.push(if anthropic {json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":data.strip_prefix("data:image/png;base64,").ok_or_else(invalid)?}})}else{json!({"type":"image_url","image_url":{"url":data}})});
    }
    message["content"] = json!(content);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn images_are_normalized_and_only_the_last_user_wire_message_receives_pixels() {
        let mut image = Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(32, 20)
            .write_to(&mut image, image::ImageFormat::Jpeg)
            .unwrap();
        let images = normalize(vec![Input {
            name: "photo.jpg".into(),
            data: format!(
                "data:image/jpeg;base64,{}",
                STANDARD.encode(image.into_inner())
            ),
        }])
        .unwrap();
        assert!(images[0].starts_with("data:image/png;base64,"));
        for anthropic in [true, false] {
            let mut body = json!({"messages":[{"role":"system","content":"system"},{"role":"user","content":"earlier"},{"role":"assistant","content":"reply"},{"role":"user","content":"current"}]});
            inject(&mut body, &images, anthropic).unwrap();
            assert_eq!(body["messages"][1]["content"], "earlier");
            assert_eq!(body["messages"][3]["content"][0]["text"], "current");
            assert_eq!(
                body["messages"][3]["content"][1]["type"],
                if anthropic { "image" } else { "image_url" }
            );
        }
        assert!(
            normalize(vec![Input {
                name: "bad.png".into(),
                data: "data:image/png;base64,AAAA".into()
            }])
            .is_err()
        );
    }
}
