// src/stores/filterStore.ts
// Media filter state (drives compute_layout re-runs)
// 媒体过滤器状态（驱动 compute_layout 重新运行）

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export const useFilterStore = defineStore('filter', () => {
  const mediaTypes = ref<string[]>([]) // empty = all
  // 空 = 全部
  const livePhotoOnly = ref(false)
  const favoritedOnly = ref(false)
  const minRating = ref(0)
  const colorLabel = ref(0) // 0=不按色筛选 / 1-7 色档（T16）
  const dateFrom = ref<number | null>(null)
  const dateTo = ref<number | null>(null)

  const hasActiveFilters = computed(
    () =>
      mediaTypes.value.length > 0 ||
      livePhotoOnly.value ||
      favoritedOnly.value ||
      minRating.value > 0 ||
      colorLabel.value > 0 ||
      // 日期范围需 from/to 两者皆备才真正下发谓词（见 toApiFilter），故"激活态"也以两者皆备为准，
      // 避免只填一端时 chip 高亮/出现「清除筛选」却实际不筛选的错觉。
      (dateFrom.value !== null && dateTo.value !== null),
  )

  function setMediaTypes(types: string[]) {
    mediaTypes.value = types
  }

  function toggleMediaType(type: string) {
    const idx = mediaTypes.value.indexOf(type)
    if (idx >= 0) {
      mediaTypes.value = mediaTypes.value.filter((t) => t !== type)
    } else {
      mediaTypes.value = [...mediaTypes.value, type]
    }
  }

  function clearFilters() {
    mediaTypes.value = []
    livePhotoOnly.value = false
    favoritedOnly.value = false
    minRating.value = 0
    colorLabel.value = 0
    dateFrom.value = null
    dateTo.value = null
  }

  function toApiFilter() {
    return {
      mediaTypes: mediaTypes.value.length ? mediaTypes.value : undefined,
      livePhotoOnly: livePhotoOnly.value || undefined,
      favoritedOnly: favoritedOnly.value || undefined,
      minRating: minRating.value > 0 ? minRating.value : undefined,
      colorLabel: colorLabel.value > 0 ? colorLabel.value : undefined,
      dateRange:
        dateFrom.value && dateTo.value ? { from: dateFrom.value, to: dateTo.value } : undefined,
    }
  }

  return {
    mediaTypes,
    livePhotoOnly,
    favoritedOnly,
    minRating,
    colorLabel,
    dateFrom,
    dateTo,
    hasActiveFilters,
    setMediaTypes,
    toggleMediaType,
    clearFilters,
    toApiFilter,
  }
})
