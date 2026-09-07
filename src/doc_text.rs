use std::fs;
use std::io::Read;
use std::path::Path;

use thiserror::Error;

const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 400_000;

#[derive(Debug, Error)]
pub enum DocTextError {
    #[error("暂不支持 {0}，请另存为 .docx / .pdf / .txt，或复制文字再贴")]
    Unsupported(String),
    #[error("文件过大（上限 20MB）")]
    TooLarge,
    #[error("无法读取文件: {0}")]
    Io(String),
    #[error("没有抽出文字。若是扫描件或图片 PDF，请复制正文再贴")]
    Empty,
}

pub fn extract_paths<P: AsRef<Path>>(paths: &[P]) -> Result<String, DocTextError> {
    if paths.is_empty() {
        return Err(DocTextError::Empty);
    }
    let mut parts = Vec::new();
    for path in paths {
        let path = path.as_ref();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("文档");
        let text = extract_path(path)?;
        if paths.len() == 1 {
            parts.push(text);
        } else {
            parts.push(format!("## {name}\n{text}"));
        }
    }
    let joined = parts.join("\n\n");
    if joined.trim().is_empty() {
        return Err(DocTextError::Empty);
    }
    Ok(truncate_text(joined))
}

pub fn extract_path(path: &Path) -> Result<String, DocTextError> {
    let meta = fs::metadata(path).map_err(|e| DocTextError::Io(e.to_string()))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(DocTextError::TooLarge);
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = match ext.as_str() {
        "txt" | "md" | "json" | "text" => read_plain(path)?,
        "docx" => extract_docx(path)?,
        "pdf" => extract_pdf(path)?,
        other => {
            let label = if other.is_empty() {
                "该文件".to_string()
            } else {
                format!(".{other}")
            };
            return Err(DocTextError::Unsupported(label));
        }
    };
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err(DocTextError::Empty);
    }
    Ok(trimmed)
}

fn read_plain(path: &Path) -> Result<String, DocTextError> {
    let bytes = fs::read(path).map_err(|e| DocTextError::Io(e.to_string()))?;
    Ok(decode_plain(&bytes))
}

fn decode_plain(bytes: &[u8]) -> String {
    let bytes = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF])
        .unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let (cow, _, _) = encoding_rs::GB18030.decode(bytes);
    cow.into_owned()
}

fn extract_docx(path: &Path) -> Result<String, DocTextError> {
    let file = fs::File::open(path).map_err(|e| DocTextError::Io(e.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| DocTextError::Io(e.to_string()))?;
    let mut xml = archive
        .by_name("word/document.xml")
        .map_err(|_| DocTextError::Io("不是有效的 .docx（缺少 document.xml）".into()))?;
    let mut raw = String::new();
    xml.read_to_string(&mut raw)
        .map_err(|e| DocTextError::Io(e.to_string()))?;
    Ok(docx_xml_to_text(&raw))
}

fn docx_xml_to_text(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(tag_at) = rest.find('<') {
        rest = &rest[tag_at..];
        if rest.starts_with("<w:p ") || rest.starts_with("<w:p>") || rest.starts_with("<w:p/") {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
        }
        if let Some(after_t) = rest.strip_prefix("<w:t") {
            let Some(gt) = after_t.find('>') else {
                break;
            };
            let start = &after_t[gt + 1..];
            let Some(end) = start.find("</w:t>") else {
                break;
            };
            out.push_str(&decode_xml_entities(&start[..end]));
            rest = &start[end + 6..];
            continue;
        }
        let Some(gt) = rest.find('>') else {
            break;
        };
        rest = &rest[gt + 1..];
    }
    out
}

fn decode_xml_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn extract_pdf(path: &Path) -> Result<String, DocTextError> {
    pdf_extract::extract_text(path).map_err(|e| DocTextError::Io(e.to_string()))
}

fn truncate_text(text: String) -> String {
    if text.chars().count() <= MAX_TEXT_CHARS {
        return text;
    }
    let mut out: String = text.chars().take(MAX_TEXT_CHARS).collect();
    out.push_str("\n\n…（文档过长，已截断）");
    out
}

#[cfg(test)]
mod tests {
    use super::{decode_plain, docx_xml_to_text, extract_path, extract_paths, DocTextError};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn txt_utf8_and_gbk() {
        let dir = TempDir::new().unwrap();
        let utf = dir.path().join("a.txt");
        std::fs::write(&utf, "接口 /api/ping").unwrap();
        assert!(extract_path(&utf).unwrap().contains("/api/ping"));

        let gbk = dir.path().join("b.txt");
        let (bytes, _, _) = encoding_rs::GB18030.encode("查询设备");
        std::fs::write(&gbk, bytes.as_ref()).unwrap();
        assert_eq!(extract_path(&gbk).unwrap(), "查询设备");
    }

    #[test]
    fn docx_reads_w_t_nodes() {
        let xml = r#"<w:document><w:body>
            <w:p><w:r><w:t>POST /hl/pub/phone</w:t></w:r></w:p>
            <w:p><w:r><w:t xml:space="preserve">出参 code</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let text = docx_xml_to_text(xml);
        assert!(text.contains("POST /hl/pub/phone"));
        assert!(text.contains("出参 code"));
    }

    #[test]
    fn docx_zip_extracts_document_xml() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("api.docx");
        write_minimal_docx(
            &path,
            r#"<w:document><w:p><w:r><w:t>GET /health</w:t></w:r></w:p></w:document>"#,
        );
        assert_eq!(extract_path(&path).unwrap(), "GET /health");
    }

    #[test]
    fn old_doc_is_unsupported() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.doc");
        std::fs::write(&path, b"x").unwrap();
        let err = extract_path(&path).unwrap_err();
        assert!(matches!(err, DocTextError::Unsupported(_)));
    }

    #[test]
    fn empty_txt_is_empty_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "   \n").unwrap();
        assert!(matches!(extract_path(&path).unwrap_err(), DocTextError::Empty));
    }

    #[test]
    fn multiple_files_join_with_names() {
        let dir = TempDir::new().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        std::fs::write(&a, "one").unwrap();
        std::fs::write(&b, "two").unwrap();
        let text = extract_paths(&[a, b]).unwrap();
        assert!(text.contains("## a.txt"));
        assert!(text.contains("one"));
        assert!(text.contains("two"));
    }

    #[test]
    fn decode_plain_strips_bom() {
        assert_eq!(decode_plain(&[0xEF, 0xBB, 0xBF, b'o', b'k']), "ok");
    }

    fn write_minimal_docx(path: &std::path::Path, document_xml: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opt = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("word/document.xml", opt).unwrap();
        zip.write_all(document_xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
}
