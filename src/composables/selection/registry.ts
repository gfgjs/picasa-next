// src/composables/selection/registry.ts
// Part5 T4a · 选择模式注册表 —— 可插拔多模式的注册/取用入口。
//
// 🔴 现在只注册 classic（当前默认起步模式）。待实测比较/市场验证时再:
//   ① 实现候选模式（可与 classic 并存做 A/B）;② 设置页暴露切换;③ config 持久化。
//   这正是「不冻结、用数据选」的落地路径 —— 加模式不触碰协议层调度、不触碰消费方、不触碰后端命令。

import type { SelectionMode } from './types'
import { classicMode } from './classicMode'

/** 模式 id → 实现。新增模式在此注册一行即可。 */
const registry = new Map<string, SelectionMode>([[classicMode.id, classicMode]])

/** 当前默认模式 id（未来从 config 读、设置页可切;现固定 classic）。 */
export const DEFAULT_MODE_ID = classicMode.id

/**
 * 取用指定模式;未注册则回退到默认模式（classic），保证调度层永不拿到 undefined。
 * @param id 模式 id;省略 → 默认模式
 */
export function getSelectionMode(id: string = DEFAULT_MODE_ID): SelectionMode {
  return registry.get(id) ?? classicMode
}

/** 已注册模式 id 列表（设置页枚举用）。 */
export function listSelectionModeIds(): string[] {
  return [...registry.keys()]
}

/**
 * 注册一个选择模式（供候选模式 / 测试 stub 注入,证明 apply 可替换、可与 classic 并存）。
 * 同 id 覆盖;返回是否为新增。
 */
export function registerSelectionMode(mode: SelectionMode): boolean {
  const isNew = !registry.has(mode.id)
  registry.set(mode.id, mode)
  return isNew
}
