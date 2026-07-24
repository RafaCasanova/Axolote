/// Retorna o Content-Type apropriado baseado na extensão do arquivo.
/// Mapeia as principais extensões da web para evitar dependência de crates externos.
pub fn get_mime_type(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        // Web Básica
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",

        // Imagens
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",

        // Mídia (Áudio e Vídeo)
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",

        // Fontes
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "eot" => "application/vnd.ms-fontobject",
        "otf" => "font/otf",

        // Documentos e Outros
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "txt" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",

        // Default
        _ => "application/octet-stream",
    }
}
