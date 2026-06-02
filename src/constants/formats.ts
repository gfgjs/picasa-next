// src/constants/formats.ts
// Supported media formats (mirrors Rust utils/format.rs)
// 支持的媒体格式（与 Rust utils/format.rs 保持一致）

export const IMAGE_FORMATS_PHASE1 = ['jpg', 'jpeg', 'png', 'webp', 'bmp', 'gif', 'tif', 'tiff'] as const

export const IMAGE_FORMATS_PHASE2 = [
  'heic', 'heif', 'avif',
  'cr2', 'cr3', 'nef', 'arw', 'dng', 'raf', 'orf', 'rw2', 'pef', 'srw',
  'psd',
] as const

export const VIDEO_FORMATS = [
  'mp4', 'm4v', 'mov', 'avi', 'mkv', 'webm', 'wmv', 'flv',
  'mpg', 'mpeg', '3gp', '3g2', 'ts', 'mts', 'm2ts', 'ogv', 'asf',
] as const

export const AUDIO_FORMATS = [
  'mp3', 'flac', 'wav', 'aac', 'm4a', 'ogg', 'oga', 'opus',
  'wma', 'aiff', 'aif', 'ape', 'alac',
] as const

export const DOCUMENT_FORMATS = [
  'pdf', 'svg', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx',
  'txt', 'md', 'rtf', 'odt', 'ods', 'odp',
] as const
