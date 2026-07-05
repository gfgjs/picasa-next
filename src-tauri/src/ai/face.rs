// src-tauri/src/ai/face.rs
//! 再导出薄壳(T16 收束):人脸检测/对齐/嵌入推理面已随进程内引擎退场,推理恒在
//! ai-worker 子进程。host 仅消费**几何纯件** `DetectedFace`(worker 派发的协议
//! FaceDet 搬运 + faces_to_records 落库映射 + quality 派生),来自 ai-core 的
//! `face_types` 模块(不在 `inference` feature 门内);引用路径不变。

pub use picasa_next_ai_core::face_types::DetectedFace;
