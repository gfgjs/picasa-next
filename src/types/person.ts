// src/types/person.ts
// People-wall / face-overlay types (F6) | 人物墙 / 人脸叠加类型（F6）

/** One person cluster card on the people wall | 人物墙上的一个人物簇卡片 */
export interface PersonSummary {
  id: number
  name: string | null
  faceCount: number
  isNamed: boolean
  isHidden: boolean
  /** Cover face geometry + its image's thumbnail (null when cover dangling) | 封面脸几何 + 其图缩略图 */
  coverItemId: number | null
  coverThumbPath: string | null
  coverThumbStatus: number | null
  /** Normalized [x, y, w, h] in [0,1] | 归一化 [x, y, w, h] */
  coverBbox: [number, number, number, number] | null
}

/** One detected face for the detail-viewer overlay | 详情查看器叠加的一张检测人脸 */
export interface FaceBox {
  id: number
  personId: number | null
  personName: string | null
  /** Normalized [x, y, w, h] in [0,1] against the image's own dimensions | 相对图像自身尺寸归一化 */
  bbox: [number, number, number, number]
  detScore: number
}

/** One unconfirmed candidate face in a likely-match group (batch approval, T10).
 *  人脸审批 likely-match 组中的一张未确认候选脸。thumbPath/thumbStatus 约定同 PersonSummary
 *  （status=1 → 相对缓存路径 / 3 → 绝对源路径），bbox 在整图缩略图内裁出脸部。 */
export interface FaceThumb {
  faceId: number
  itemId: number
  thumbPath: string | null
  thumbStatus: number | null
  /** Normalized [x, y, w, h] in [0,1] | 归一化 [x, y, w, h] */
  bbox: [number, number, number, number]
  /** This face's cosine similarity to the candidate person's centroid | 与候选人物质心的余弦相似度 */
  similarity: number
}

/** A group of unconfirmed faces tentatively assigned to ONE candidate person (batch approval, T10).
 *  一组暂归于同一候选人物的未确认脸,供批量审批整组一次性确认/改派/拒绝。 */
export interface LikelyMatchGroup {
  personId: number
  personName: string | null
  candidateFaces: FaceThumb[]
  /** Mean per-face similarity = group match strength | 单脸相似度均值 = 组匹配强度 */
  confidence: number
}
