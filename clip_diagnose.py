"""
Chinese-CLIP 诊断脚本：对比 Python Transformers vs ONNX Runtime 的输出。
用法: python clip_diagnose.py

这个脚本会：
1. 检查 ONNX 模型的输入/输出规格
2. 用 Python transformers 分词器生成 token IDs，对比 Rust 应该产生的结果
3. 用 ONNX Runtime 运行推理，检查嵌入向量是否合理
4. 测试文本-图像相似度
"""

import os
import sys
import numpy as np

MODELS_DIR = os.path.expandvars(r"%APPDATA%\com.picasanext.app\models")
IMAGE_MODEL = os.path.join(MODELS_DIR, "vit-b-16.img.fp16.onnx")
TEXT_MODEL  = os.path.join(MODELS_DIR, "vit-b-16.txt.fp16.onnx")
VOCAB_FILE  = os.path.join(MODELS_DIR, "vocab.txt")

def check_files():
    print("=" * 60)
    print("1. 检查模型文件")
    print("=" * 60)
    for f in [IMAGE_MODEL, TEXT_MODEL, VOCAB_FILE]:
        exists = os.path.exists(f)
        size = os.path.getsize(f) / 1024 / 1024 if exists else 0
        print(f"  {'✓' if exists else '✗'} {os.path.basename(f)} ({size:.2f} MB)")
    print()

def inspect_onnx_models():
    """检查 ONNX 模型的输入输出规格"""
    print("=" * 60)
    print("2. ONNX 模型输入/输出规格")
    print("=" * 60)
    import onnxruntime as ort
    
    for name, path in [("Image Encoder", IMAGE_MODEL), ("Text Encoder", TEXT_MODEL)]:
        if not os.path.exists(path):
            print(f"  ✗ {name}: 文件不存在")
            continue
        sess_opts = ort.SessionOptions()
        sess_opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_BASIC
        sess = ort.InferenceSession(path, sess_opts, providers=["CPUExecutionProvider"])
        print(f"\n  {name}: {os.path.basename(path)}")
        print(f"  输入:")
        for inp in sess.get_inputs():
            print(f"    - {inp.name}: {inp.type} {inp.shape}")
        print(f"  输出:")
        for out in sess.get_outputs():
            print(f"    - {out.name}: {out.type} {out.shape}")
    print()

def test_tokenizer():
    """对比 Python transformers tokenizer 和预期的 Rust tokenizer"""
    print("=" * 60)
    print("3. Tokenizer 输出对比")
    print("=" * 60)
    
    # 加载 vocab.txt 手动分词（模拟 Rust 的 tokenizers crate）
    vocab = {}
    with open(VOCAB_FILE, "r", encoding="utf-8") as f:
        for i, line in enumerate(f):
            vocab[line.strip()] = i
    print(f"  Vocab 大小: {len(vocab)}")
    print(f"  [CLS]={vocab.get('[CLS]', 'MISSING')}, [SEP]={vocab.get('[SEP]', 'MISSING')}, [PAD]={vocab.get('[PAD]', 'MISSING')}, [UNK]={vocab.get('[UNK]', 'MISSING')}")
    
    # 用 transformers 分词
    try:
        from transformers import BertTokenizer
        tokenizer = BertTokenizer(vocab_file=VOCAB_FILE, do_lower_case=True)
        
        test_texts = ["小猫", "花朵", "一只白色的猫在沙发上睡觉", "海边日落", "森林", "人物肖像"]
        for text in test_texts:
            enc = tokenizer(text, max_length=52, padding="max_length", truncation=True, return_tensors="np")
            ids = enc["input_ids"][0]
            mask = enc["attention_mask"][0]
            # 只显示非零部分
            non_pad = ids[ids != 0]
            print(f"\n  \"{text}\":")
            print(f"    input_ids (非PAD): {non_pad.tolist()}")
            print(f"    attention_mask sum: {mask.sum()} (= token count)")
            # 解码回来验证
            decoded = tokenizer.decode(non_pad, skip_special_tokens=True)
            print(f"    decoded: \"{decoded}\"")
    except ImportError:
        print("  ✗ transformers 未安装，跳过分词对比")
        print("    pip install transformers")
    print()

def test_text_inference():
    """用 ONNX Runtime 测试文本编码"""
    print("=" * 60)
    print("4. 文本编码器推理测试")
    print("=" * 60)
    
    import onnxruntime as ort
    
    if not os.path.exists(TEXT_MODEL):
        print("  ✗ 文本模型不存在")
        return None
    
    sess_opts = ort.SessionOptions()
    sess_opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_BASIC
    sess = ort.InferenceSession(TEXT_MODEL, sess_opts, providers=["CPUExecutionProvider"])
    input_names = [inp.name for inp in sess.get_inputs()]
    output_names = [out.name for out in sess.get_outputs()]
    print(f"  输入名: {input_names}")
    print(f"  输出名: {output_names}")
    
    try:
        from transformers import BertTokenizer
        tokenizer = BertTokenizer(vocab_file=VOCAB_FILE, do_lower_case=True)
    except ImportError:
        print("  ✗ transformers 未安装")
        return None
    
    text_embeddings = {}
    test_texts = ["小猫", "花朵", "海边日落", "一只白色的猫在沙发上睡觉"]
    
    for text in test_texts:
        enc = tokenizer(text, max_length=52, padding="max_length", truncation=True, return_tensors="np")
        
        # 构建输入 — 根据模型实际需要的输入
        feeds = {}
        if "text" in input_names:
            feeds["text"] = enc["input_ids"].astype(np.int64)
        if "input_ids" in input_names:
            feeds["input_ids"] = enc["input_ids"].astype(np.int64)
        if "attention_mask" in input_names:
            feeds["attention_mask"] = enc["attention_mask"].astype(np.int64)
        if "token_type_ids" in input_names:
            feeds["token_type_ids"] = enc["token_type_ids"].astype(np.int64)
        
        print(f"\n  \"{text}\":")
        print(f"    实际传入的输入: {list(feeds.keys())}")
        
        outputs = sess.run(output_names, feeds)
        feat = outputs[0][0]  # [512]
        
        # L2 归一化
        norm = np.linalg.norm(feat)
        feat_normed = feat / (norm + 1e-8)
        
        print(f"    原始 L2 范数: {norm:.4f}")
        print(f"    前 8 维: {feat_normed[:8].tolist()}")
        text_embeddings[text] = feat_normed
    
    # 文本间相似度矩阵
    print("\n  文本间余弦相似度:")
    texts = list(text_embeddings.keys())
    for i, t1 in enumerate(texts):
        for j, t2 in enumerate(texts):
            if j > i:
                sim = np.dot(text_embeddings[t1], text_embeddings[t2])
                print(f"    \"{t1}\" ↔ \"{t2}\": {sim:.4f}")
    
    print()
    return text_embeddings

def test_image_inference(test_image_path=None):
    """用 ONNX Runtime 测试图像编码"""
    print("=" * 60)
    print("5. 图像编码器推理测试")
    print("=" * 60)
    
    import onnxruntime as ort
    
    if not os.path.exists(IMAGE_MODEL):
        print("  ✗ 图像模型不存在")
        return None
    
    sess_opts = ort.SessionOptions()
    sess_opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_BASIC
    sess = ort.InferenceSession(IMAGE_MODEL, sess_opts, providers=["CPUExecutionProvider"])
    input_names = [inp.name for inp in sess.get_inputs()]
    output_names = [out.name for out in sess.get_outputs()]
    print(f"  输入名: {input_names}")
    print(f"  输出名: {output_names}")
    
    if test_image_path and os.path.exists(test_image_path):
        from PIL import Image
        
        # 使用官方 Chinese-CLIP 的预处理
        img = Image.open(test_image_path).convert("RGB")
        print(f"\n  测试图: {test_image_path} ({img.size[0]}x{img.size[1]})")
        
        # 官方预处理: Resize(224, BICUBIC) → CenterCrop(224) → Normalize
        from torchvision import transforms
        transform = transforms.Compose([
            transforms.Resize(224, interpolation=transforms.InterpolationMode.BICUBIC),
            transforms.CenterCrop(224),
            transforms.ToTensor(),
            transforms.Normalize(
                mean=[0.48145466, 0.4578275, 0.40821073],
                std=[0.26862954, 0.26130258, 0.27577711]
            ),
        ])
        
        tensor = transform(img).unsqueeze(0).numpy().astype(np.float32)
        
        feeds = {}
        if "image" in input_names:
            feeds["image"] = tensor
        elif "pixel_values" in input_names:
            feeds["pixel_values"] = tensor
        
        outputs = sess.run(output_names, feeds)
        feat = outputs[0][0]
        norm = np.linalg.norm(feat)
        feat_normed = feat / (norm + 1e-8)
        
        print(f"    原始 L2 范数: {norm:.4f}")
        print(f"    前 8 维: {feat_normed[:8].tolist()}")
        return feat_normed
    else:
        print("  跳过图像测试（未提供测试图片路径）")
        print("  用法: python clip_diagnose.py <测试图片路径>")
        return None

def test_cross_modal(text_embeddings, image_embedding):
    """测试文本-图像跨模态相似度"""
    if text_embeddings is None or image_embedding is None:
        return
    
    print("=" * 60)
    print("6. 文本-图像跨模态相似度")
    print("=" * 60)
    
    for text, text_emb in text_embeddings.items():
        sim = np.dot(text_emb, image_embedding)
        print(f"  \"{text}\" ↔ 图像: {sim:.4f}")
    print()

def main():
    check_files()
    inspect_onnx_models()
    test_tokenizer()
    text_embs = test_text_inference()
    
    img_path = sys.argv[1] if len(sys.argv) > 1 else None
    img_emb = test_image_inference(img_path)
    
    test_cross_modal(text_embs, img_emb)
    
    print("=" * 60)
    print("诊断完成。请将以上输出发给我分析。")
    print("=" * 60)

if __name__ == "__main__":
    main()
