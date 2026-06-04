// src/stores/filterStore.ts
// Media filter state (drives compute_layout re-runs)
// 媒体过滤器状态（驱动 compute_layout 重新运行）

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import type { MediaType } from '../types/media'

export const useFilterStore = defineStore('filter', () => {
  const mediaTypes    = ref<MediaType[]>([])  // empty = all
                                           // 空 = 全部
  const livePhotoOnly = ref(false)
  const favoritedOnly = ref(false)
  const minRating     = ref(0)
  const dateFrom      = ref<number | null>(null)
  const dateTo        = ref<number | null>(null)

  const hasActiveFilters = computed(() =>
    mediaTypes.value.length > 0 ||
    livePhotoOnly.value ||
    favoritedOnly.value ||
    minRating.value > 0 ||
    dateFrom.value !== null
  )

  function setMediaTypes(types: MediaType[]) {
    mediaTypes.value = types
  }

  function toggleMediaType(type: MediaType) {
    const idx = mediaTypes.value.indexOf(type)
    if (idx >= 0) {
      mediaTypes.value = mediaTypes.value.filter(t => t !== type)
    } else {
      mediaTypes.value = [...mediaTypes.value, type]
    }
  }

  function clearFilters() {
    mediaTypes.value    = []
    livePhotoOnly.value = false
    favoritedOnly.value = false
    minRating.value     = 0
    dateFrom.value      = null
    dateTo.value        = null
  }

  function toApiFilter(): import('../types/media').MediaFilter {
    return {
      mediaTypes:    mediaTypes.value.length ? mediaTypes.value : undefined,
      livePhotoOnly: livePhotoOnly.value || undefined,
      favoritedOnly: favoritedOnly.value || undefined,
      minRating:     minRating.value > 0 ? minRating.value : undefined,
      dateRange:     dateFrom.value && dateTo.value
        ? { from: dateFrom.value, to: dateTo.value }
        : undefined,
    }
  }

  return {
    mediaTypes, livePhotoOnly, favoritedOnly, minRating, dateFrom, dateTo,
    hasActiveFilters,
    setMediaTypes, toggleMediaType, clearFilters, toApiFilter,
  }
})
