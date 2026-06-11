import os
import re

def process_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    orig = content

    # Lock errors
    content = re.sub(r'\.lock\(\)\.map_err\(\|e\| AppError::Db\(e\.to_string\(\)\)\)', 
                     r'.lock().map_err(|e| AppError::System(e.to_string()))', content)
    content = re.sub(r'AppError::Db\(format!\("Lock error: [^"]+", e, e\)\)', 
                     r'AppError::System(format!("Lock error: {}", e))', content)
    content = re.sub(r'AppError::Db\(format!\("Lock error: \{e\} \| 锁错误: \{e\}"\)\)', 
                     r'AppError::System(format!("Lock error: {e}"))', content)

    # e.to_string() mappings
    content = re.sub(r'AppError::Db\(e\.to_string\(\)\)', r'AppError::Db(e)', content)
    content = re.sub(r'AppError::Io\(e\.to_string\(\)\)', r'AppError::Io(e)', content)
    content = re.sub(r'AppError::Engine\(e\.to_string\(\)\)', r'AppError::Engine(e)', content)
    content = re.sub(r'AppError::Ai\(e\.to_string\(\)\)', r'AppError::Ai(e)', content)
    content = re.sub(r'AppError::Pool\(e\.to_string\(\)\)', r'AppError::Pool(e)', content)

    # Specific formatted errors -> pass `e`
    content = re.sub(r'AppError::Db\(format!\("Migration v\d failed: \{e\}"\)\)', r'AppError::Db(e)', content)
    content = re.sub(r'AppError::Io\(format!\("Failed to remove thumbnail cache: \{e\}"\)\)', r'AppError::Io(e)', content)
    content = re.sub(r'AppError::Io\(format!\("Cannot create models dir \| 无法创建模型目录: \{e\}"\)\)', r'AppError::Io(e)', content)

    # Engine Os errors
    content = re.sub(r'AppError::Engine\(format!\("Failed to set wallpaper: \{\}", e\)\)', r'AppError::Os(format!("Failed to set wallpaper: {}", e))', content)
    content = re.sub(r'AppError::Engine\(format!\("Failed to set wallpaper mode: \{\}", e\)\)', r'AppError::Os(format!("Failed to set wallpaper mode: {}", e))', content)
    content = re.sub(r'AppError::Engine\(format!\("Failed to open image for clipboard: \{\}", e\)\)', r'AppError::Engine(e)', content)
    content = re.sub(r'AppError::Engine\(format!\("Failed to initialize clipboard: \{\}", e\)\)', r'AppError::Os(format!("Failed to initialize clipboard: {}", e))', content)
    content = re.sub(r'AppError::Engine\(format!\("Failed to set clipboard image: \{\}", e\)\)', r'AppError::Os(format!("Failed to set clipboard image: {}", e))', content)
    content = re.sub(r'AppError::Engine\(format!\("WIC [^"]+"\)\)', r'AppError::Os(format!("WIC error: {}", e))', content)

    # AI formatted -> Ai(e) or AiTokenizer
    content = re.sub(r'AppError::Ai\(format!\("Tokenizer build failed \| 分词器构建失败: \{e\}"\)\)', r'AppError::AiTokenizer(format!("Tokenizer error: {e}"))', content)
    content = re.sub(r'AppError::Ai\(format!\("Tokenize failed \| 分词失败: \{e\}"\)\)', r'AppError::AiTokenizer(format!("Tokenize failed: {e}"))', content)
    
    # Generic format capturing `e` for `ort::Error`
    content = re.sub(r'AppError::Ai\(format!\("[^"]+ \| [^"]+: \{e\}"\)\)', r'AppError::Ai(e)', content)
    content = re.sub(r'AppError::Ai\(format!\("Template [^"]+: \{e\}"\)\)', r'AppError::Ai(e)', content)

    # String literal variants
    content = re.sub(r'AppError::Db\(format!\("spawn_blocking join error: \{e\}"\)\)', r'AppError::System(format!("spawn_blocking join error: {e}"))', content)
    content = re.sub(r'AppError::Ai\("Session pool disconnected"\.into\(\)\)', r'AppError::Internal("Session pool disconnected".into())', content)
    content = re.sub(r'AppError::Ai\("CLIP engine not initialized \| CLIP 引擎未初始化"\.to_string\(\)\)', r'AppError::Internal("CLIP engine not initialized".into())', content)
    content = re.sub(r'AppError::Engine\("embedding cache missing"\.into\(\)\)', r'AppError::Internal("embedding cache missing".into())', content)
    content = re.sub(r'AppError::Engine\("No embedded MP4 found in Motion Photo"\.into\(\)\)', r'AppError::Internal("No embedded MP4 found in Motion Photo".into())', content)
    content = re.sub(r'AppError::Engine\("WebP encode failed"\.into\(\)\)', r'AppError::Internal("WebP encode failed".into())', content)
    content = re.sub(r'AppError::Engine\("resize buffer mismatch"\.into\(\)\)', r'AppError::Internal("resize buffer mismatch".into())', content)
    content = re.sub(r'AppError::Ai\(format!\("Batch output tensor out of bounds \| 批处理输出张量越界: start=\{\}, end=\{\}, len=\{\}", start, end, raw_slice\.len\(\)\)\)', r'AppError::Internal(format!("Batch output tensor out of bounds"))', content)

    # Multiline replacements
    multiline_vocab = r'''AppError::Ai\(format!\(
                "Wrong vocab\.txt: only \{\} tokens, expected ~21128\. \\
                 Please replace with bert-base-chinese vocab from \\
                 OFA-Sys/chinese-clip-vit-base-patch16\. \\
                 Got vocab size: \{\}",
                vocab_size, vocab_size
            \)\)'''
    content = re.sub(multiline_vocab, r'AppError::AiTokenizer("Wrong vocab.txt".to_string())', content, flags=re.MULTILINE)

    if orig != content:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"Updated {filepath}")

for root, _, files in os.walk(r'D:\photoapp\picasa-next\src-tauri\src'):
    for f in files:
        if f.endswith('.rs'):
            process_file(os.path.join(root, f))
