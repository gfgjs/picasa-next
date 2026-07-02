# 验 SFace 预处理（纯 cv2，避开崩溃的 onnxruntime）：给定 OpenCV 对齐图，用 cv2.dnn 手动喂
# 不同预处理的 blob，对比 cv2.FaceRecognizerSF.feature()。cosine≈1 的那种即正确预处理。
import sys, cv2, numpy as np
lena = sys.argv[1]; models = sys.argv[2]
yunet = models + "/face_detection_yunet_2023mar.onnx"
sface = models + "/face_recognition_sface_2021dec.onnx"

img = cv2.imread(lena); h, w = img.shape[:2]
det = cv2.FaceDetectorYN.create(yunet, "", (w, h), 0.6, 0.3, 50); det.setInputSize((w, h))
_, faces = det.detect(img); face = faces[0]
rec = cv2.FaceRecognizerSF.create(sface, "")
aligned = rec.alignCrop(img, face)            # BGR uint8 112x112（基准对齐图）
feat_cv = rec.feature(aligned).flatten(); feat_cv = feat_cv / np.linalg.norm(feat_cv)

net = cv2.dnn.readNetFromONNX(sface)
def run(blob):
    net.setInput(blob.astype(np.float32).copy()); o = net.forward().flatten(); return o/np.linalg.norm(o)

bgr   = run(aligned.transpose(2,0,1)[None])                          # 我的：BGR 0-255
rgb   = run(aligned[...,::-1].transpose(2,0,1)[None])                # RGB 0-255
bgr_n = run(((aligned.astype(np.float32)-127.5)/128.0).transpose(2,0,1)[None])  # BGR (x-127.5)/128

print("BGR/0-255 vs feature :", round(float(bgr@feat_cv),4))
print("RGB/0-255 vs feature :", round(float(rgb@feat_cv),4))
print("BGR/norm  vs feature :", round(float(bgr_n@feat_cv),4))
