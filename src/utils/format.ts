// src/utils/format.ts
// Frontend formatting utilities
// 前端格式化工具

/**
 * Format bytes to a human-readable string (e.g. "4.2 MB").
 * 格式化字节数为人类可读的字符串（例如 "4.2 MB"）。
 */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024)             return `${bytes} B`
  if (bytes < 1024 * 1024)      return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

/**
 * Format a Unix timestamp (seconds) to a locale date string.
 * 将 Unix 时间戳（秒）格式化为本地日期字符串。
 */
export function formatDate(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString('zh-CN', {
    year:  'numeric',
    month: 'long',
    day:   'numeric',
  })
}

/**
 * Format a Unix timestamp to date + time.
 * 将 Unix 时间戳格式化为日期 + 时间。
 */
export function formatDateTime(ts: number): string {
  return new Date(ts * 1000).toLocaleString('zh-CN', {
    year:   'numeric',
    month:  'long',
    day:    'numeric',
    hour:   '2-digit',
    minute: '2-digit',
  })
}

/**
 * Format a duration in milliseconds to "mm:ss" or "h:mm:ss".
 * 将毫秒持续时间格式化为 "mm:ss" 或 "h:mm:ss"。
 */
export function formatDuration(ms: number): string {
  const totalSec = Math.floor(ms / 1000)
  const hours = Math.floor(totalSec / 3600)
  const mins  = Math.floor((totalSec % 3600) / 60)
  const secs  = totalSec % 60

  const pad = (n: number) => String(n).padStart(2, '0')

  if (hours > 0) return `${hours}:${pad(mins)}:${pad(secs)}`
  return `${mins}:${pad(secs)}`
}

/**
 * Format an aperture value to "f/1.8" style.
 * 将光圈值格式化为 "f/1.8" 样式。
 */
export function formatAperture(aperture: number): string {
  return `f/${aperture.toFixed(1)}`
}

/**
 * Format focal length to "35mm" style.
 * 将焦距格式化为 "35mm" 样式。
 */
export function formatFocalLength(mm: number): string {
  return `${mm.toFixed(0)}mm`
}

/**
 * Format GPS coordinates to "40.7128° N, 74.0060° W".
 * 将 GPS 坐标格式化为 "40.7128° N, 74.0060° W"。
 */
export function formatGps(lat: number, lng: number): string {
  const latDir = lat >= 0 ? 'N' : 'S'
  const lngDir = lng >= 0 ? 'E' : 'W'
  return `${Math.abs(lat).toFixed(4)}° ${latDir}, ${Math.abs(lng).toFixed(4)}° ${lngDir}`
}

/**
 * Resolve a thumbnail absolute path via Tauri's convertFileSrc.
 * Returns empty string if path is null.
 * 通过 Tauri 的 convertFileSrc 解析缩略图绝对路径。
 * 如果路径为 null，则返回空字符串。
 */
export async function thumbUrl(
  thumbPath: string | null,
  cacheDir: string,
  size: number = 300,
): Promise<string> {
  if (!thumbPath) return ''
  const { convertFileSrc } = await import('@tauri-apps/api/core')
  const abs = `${cacheDir}/thumbnails/${thumbPath}`.replace(/\\/g, '/')
  return convertFileSrc(abs)
}

/**
 * Get a badge label for a media type.
 * 获取媒体类型的徽章标签。
 */
export function mediaBadgeLabel(mediaType: string, isLivePhoto: boolean): string | null {
  if (mediaType === 'image' && isLivePhoto) return 'LIVE'
  if (mediaType === 'video')                return '▶'
  if (mediaType === 'audio')                return '♪'
  if (mediaType === 'document')             return 'DOC'
  return null
}
