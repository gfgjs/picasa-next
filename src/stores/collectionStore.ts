// src/stores/collectionStore.ts
// Collections / favorites state (需求7, §3.7).
// 收藏夹状态（需求7, §3.7）。
//
// Backed by the backend `albums`/`album_items` tables. System folders (4 seeded type
// folders) are virtual (type + is_favorited); user folders store real membership.
// 由后端 albums/album_items 承载。系统夹（4 个播种类型夹）虚拟（类型 + is_favorited）；
// 用户夹存实体成员。

import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { IPC } from '../constants/ipc'
import type { Collection } from '../types/media'

export const useCollectionStore = defineStore('collection', () => {
  const collections = ref<Collection[]>([])
  const isLoading = ref(false)

  /** Load all collections (system folders first, then user). | 加载全部收藏夹。 */
  async function load() {
    isLoading.value = true
    try {
      collections.value = await invoke<Collection[]>(IPC.LIST_COLLECTIONS)
    } catch (e) {
      console.error('[CollectionStore] load failed:', e)
    } finally {
      isLoading.value = false
    }
  }

  /** Recently-used user collections, for the favorite toast chips. | 最近使用的用户收藏夹（toast chips）。 */
  async function recent(limit = 5): Promise<Collection[]> {
    try {
      return await invoke<Collection[]>(IPC.RECENT_COLLECTIONS, { limit })
    } catch (e) {
      console.error('[CollectionStore] recent failed:', e)
      return []
    }
  }

  /** Create a user collection; refreshes the list. Returns its id. | 新建用户收藏夹并刷新列表。 */
  async function create(name: string, icon?: string): Promise<number | null> {
    try {
      const id = await invoke<number>(IPC.CREATE_COLLECTION, { name, icon: icon ?? null })
      await load()
      return id
    } catch (e) {
      console.error('[CollectionStore] create failed:', e)
      return null
    }
  }

  /** Delete a user collection; refreshes the list. | 删除用户收藏夹并刷新列表。 */
  async function remove(albumId: number) {
    try {
      await invoke(IPC.DELETE_COLLECTION, { albumId })
      await load()
    } catch (e) {
      console.error('[CollectionStore] remove failed:', e)
    }
  }

  /** Rename a user collection (system folders protected backend-side); updates in place.
   *  重命名用户收藏夹（系统夹由后端守卫保护）；原地更新列表。 */
  async function rename(albumId: number, name: string) {
    try {
      await invoke(IPC.RENAME_COLLECTION, { albumId, name })
      const c = collections.value.find((c) => c.id === albumId)
      if (c) c.name = name
    } catch (e) {
      console.error('[CollectionStore] rename failed:', e)
    }
  }

  /** Add items to a user collection. Returns rows inserted. | 向用户收藏夹添加项。 */
  async function addItems(albumId: number, itemIds: number[]): Promise<number> {
    try {
      return await invoke<number>(IPC.ADD_TO_COLLECTION, { albumId, itemIds })
    } catch (e) {
      console.error('[CollectionStore] addItems failed:', e)
      return 0
    }
  }

  /** Remove items from a collection. Returns rows deleted. | 从收藏夹移除项。 */
  async function removeItems(albumId: number, itemIds: number[]): Promise<number> {
    try {
      return await invoke<number>(IPC.REMOVE_FROM_COLLECTION, { albumId, itemIds })
    } catch (e) {
      console.error('[CollectionStore] removeItems failed:', e)
      return 0
    }
  }

  return { collections, isLoading, load, recent, create, rename, remove, addItems, removeItems }
})
