import os
import re

def process_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    orig = content

    # 1. thumbnail_commands.rs:40 pool get error
    if filepath.endswith("thumbnail_commands.rs"):
        content = re.sub(r'\.get\(\)\.map_err\(\|e\| AppError::Db\(e\)\)', r'.get().map_err(AppError::from)', content)
        content = re.sub(r'\.map_err\(\|e\| AppError::Io\(e\)\)\?', r'.map_err(|e| AppError::Io(e.into()))?', content)
    
    # 3. clip.rs
    if filepath.endswith("clip.rs"):
        content = re.sub(r'AppError::Engine\(format!\("Image decode failed \| 图像解码失败: \{e\}"\)\)', r'AppError::Internal(format!("Image decode failed | 图像解码失败: {e}"))', content)
        # 424 multiline
        content = re.sub(r'return Err\(AppError::Ai\(format!\([^;]+;\n?', r'return Err(AppError::AiTokenizer("Wrong vocab.txt".to_string()));\n', content, flags=re.MULTILINE | re.DOTALL)
        
        # specific fixes for clip.rs
        content = re.sub(r'AppError::Ai\(format!\("Template single failed: \{e\}"\)\)', r'AppError::AiTokenizer(format!("Template single failed: {e}"))', content)
        content = re.sub(r'AppError::Ai\(format!\("Template pair failed: \{e\}"\)\)', r'AppError::AiTokenizer(format!("Template pair failed: {e}"))', content)
        content = re.sub(r'AppError::Ai\(format!\("Post-processor build failed \| 后处理器构建失败: \{e\}"\)\)', r'AppError::AiTokenizer(format!("Post-processor build failed | 后处理器构建失败: {e}"))', content)
        content = re.sub(r'AppError::Ai\(format!\("Truncation config failed \| 截断配置失败: \{e\}"\)\)', r'AppError::AiTokenizer(format!("Truncation config failed | 截断配置失败: {e}"))', content)
        
        # Some are already Ai(e) from the previous python script! Let's revert or fix them:
        # Template single failed was previously converted to AppError::Ai(e) in fix_errors.py
        # We know the line numbers or approximate context. Let's just fix them directly by replacing `Ai(e)` with `AiTokenizer(e.to_string())` for the ones that are NOT `ort::Error`.
        # ort::Error ones:
        # .map_err(|e| AppError::Ai(e))? -> where it was ort::Error
        # Actually it's easier to use replace_file_content for clip.rs. I'll skip it here.
        pass
    
    # 7. engine/image_rs.rs
    if filepath.endswith("image_rs.rs"):
        content = re.sub(r'\.map_err\(\|e\| AppError::Engine\(e\)\)\?', r'.map_err(AppError::Io)?', content)

    # 8. engine/gpu/wic_engine.rs
    if filepath.endswith("wic_engine.rs"):
        content = re.sub(r'AppError::Engine\(format!\("Failed to create WIC factory: \{\}", e\)\)', r'AppError::Os(format!("Failed to create WIC factory: {}", e))', content)

    if orig != content and not filepath.endswith("clip.rs"):
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"Updated {filepath}")

for root, _, files in os.walk(r'D:\photoapp\picasa-next\src-tauri\src'):
    for f in files:
        if f.endswith('.rs'):
            process_file(os.path.join(root, f))
