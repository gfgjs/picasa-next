<template>
  <div class="settings-card__item">
    <button
      class="pin-btn"
      :class="{ active: ui.pinnedSettings.includes(settingKey) }"
      @click="ui.togglePinnedSetting(settingKey)"
      :title="$t('settings.pinToSidebar')"
      :aria-label="$t('settings.pinToSidebar')"
    >
      <Pin :size="14" />
    </button>
    <div class="settings-card__info">
      <div class="settings-card__label">{{ $t(spec.label) }}</div>
      <!-- 描述槽:特例行(可点击路径等)整体替换;默认取注册表 descKey -->
      <slot name="desc">
        <div v-if="spec.descKey" class="settings-card__desc">{{ $t(spec.descKey) }}</div>
      </slot>
      <!-- 描述下方附加块(如全量生成进度条) -->
      <slot name="extra" />
    </div>
    <!-- 默认控件=注册表驱动的 DynamicSettingControl;特例行可整体替换,no-control 则省略 -->
    <slot>
      <DynamicSettingControl v-if="!noControl" :setting-key="settingKey" />
    </slot>
  </div>
</template>

<script setup lang="ts">
// 设置行外壳(设计 §8 注册式重构):pin 钮 + 标签/描述(i18n key 来自 SETTINGS_MAP)+ 控件槽。
// DOM 结构与类名和重构前 SettingsView 的手写行逐一等价(settings-card__* 基础样式在 index.css)。
import { computed } from 'vue'
import { Pin } from '@lucide/vue'
import { useUiStore } from '../../stores/uiStore'
import { SETTINGS_MAP } from '../../constants/settingsMap'
import DynamicSettingControl from './DynamicSettingControl.vue'

const props = defineProps<{
  /** 注册表键;pin 状态、标签/描述 i18n key 均由它检索。 */
  settingKey: string
  /** true=本行无右侧控件(如主题行,其控件为下方 ThemePicker)。 */
  noControl?: boolean
}>()

const ui = useUiStore()
const spec = computed(() => SETTINGS_MAP[props.settingKey])
</script>

<style scoped>
/* __item/__info/__label/__desc 基础样式在 index.css(全局);此处仅行外壳特有部分。 */
.settings-card__item {
  padding-left: 12px;
}

.pin-btn {
  background: transparent;
  border: none;
  color: var(--color-text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all var(--transition-fast);
}
.pin-btn:hover {
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
}
.pin-btn.active {
  /* 原 var(--color-primary) 为幽灵 token(六套主题均未定义,激活态一直没渲染出来;
     S6 顺带修):按语义落 accent。 */
  color: var(--color-accent);
  background: color-mix(in srgb, var(--color-accent) 10%, transparent);
}
</style>
