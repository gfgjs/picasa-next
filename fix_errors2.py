import os
import re

def process_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    orig = content

    # WIC errors -> Os
    content = re.sub(r'AppError::Engine\(format!\("WIC [^"]+", e\)\)', r'AppError::Os(format!("WIC error: {}", e))', content)

    # Exif thumb -> Internal
    content = re.sub(r'AppError::Engine\(format!\("WebP encode failed: \{e\}"\)\)', r'AppError::Internal(format!("WebP encode failed: {e}"))', content)
    content = re.sub(r'AppError::Engine\(format!\("JPEG encode failed: \{e\}"\)\)', r'AppError::Internal(format!("JPEG encode failed: {e}"))', content)

    # generator.rs -> Internal
    content = re.sub(r'AppError::Engine\(format!\("panic during \{label\}"\)\)', r'AppError::Internal(format!("panic during {label}"))', content)
    content = re.sub(r'AppError::Engine\(format!\("Unknown GPU engine: \{\}", config\.gpu_engine\)\)', r'AppError::Internal(format!("Unknown GPU engine: {}", config.gpu_engine))', content)

    # Revert specific AppError::Engine(e) which should be AppError::Internal(e.to_string()) because they are from fast_image_resize
    # In src/thumbnail/generator.rs and src/thumbnail/thumbhash.rs
    if filepath.endswith("generator.rs") or filepath.endswith("thumbhash.rs"):
        content = re.sub(r'\.map_err\(\|e\| AppError::Engine\(e\)\)\?', r'.map_err(|e| AppError::Internal(e.to_string()))?', content)

    if orig != content:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"Updated {filepath}")

for root, _, files in os.walk(r'D:\photoapp\picasa-next\src-tauri\src'):
    for f in files:
        if f.endswith('.rs'):
            process_file(os.path.join(root, f))
