<!-- src/views/PluginStoreView.vue -->
<!-- 插件商店（Part5 T11）：浏览签名 Registry 可安装条目 + 安装生命周期 + 处理进度。 -->
<!-- Plugin store (Part5 T11): browse signed registry, install lifecycle, processing progress. -->
<!--
  🔴 开源/闭源边界（Part0 §10）：本视图只调后端命令（列表 / 安装 / 激活 / 进度），
     验签 / 防回滚 / 完整性校验全在后端；命令只传已校验 pluginId，不碰下载坐标，不持验签逻辑。
  数据/动作经 useExoticStore；激活复用 ExoticActivateDialog。
-->
<template>
  <div class="plugin-store">
    <header class="ps-header">
      <div class="ps-header__text">
        <h2 class="ps-title">{{ $t('exotic.storeTitle') }}</h2>
        <p class="ps-subtitle">{{ $t('exotic.storeSubtitle') }}</p>
      </div>
      <button class="btn btn-primary" :disabled="refreshing" @click="onRefresh">
        <RefreshCw :size="15" :class="{ 'spin-anim': refreshing }" />
        {{ refreshing ? $t('exotic.storeRefreshing') : $t('exotic.storeRefresh') }}
      </button>
    </header>

    <!-- 目录过期横幅：仍可展示但不允许新装。 -->
    <div v-if="anyExpired" class="ps-banner ps-banner--warn">
      {{ $t('exotic.storeExpiredHint') }}
    </div>

    <!-- 处理进度：有 exotic 任务时才显（进度条 + 计数 + 控制）。 -->
    <section v-if="proc && procTotal > 0" class="ps-proc">
      <div class="ps-proc__head">
        <span class="ps-proc__title">{{ $t('exotic.procTitle') }}</span>
        <div class="ps-proc__ctrls">
          <button
            v-if="!proc.running"
            class="btn btn-ghost btn-sm"
            :disabled="procBusy"
            @click="ctrl(store.startProcessing)"
          >
            <Play :size="14" /> {{ $t('exotic.procStart') }}
          </button>
          <button
            v-else
            class="btn btn-ghost btn-sm"
            :disabled="procBusy"
            @click="ctrl(store.pauseProcessing)"
          >
            <Pause :size="14" /> {{ $t('exotic.procPause') }}
          </button>
          <button
            class="btn btn-ghost btn-sm"
            :disabled="procBusy || (!proc.running && proc.processing === 0)"
            @click="ctrl(store.stopProcessing)"
          >
            <Square :size="14" /> {{ $t('exotic.procStop') }}
          </button>
        </div>
      </div>
      <div class="ps-proc__bar">
        <div class="ps-proc__bar-fill" :style="{ width: procDonePct + '%' }" />
      </div>
      <div class="ps-proc__meta">
        <span class="ps-proc__count">{{ proc.done }} / {{ procTotal }}</span>
        <span v-if="proc.blockedByAvailability > 0" class="ps-proc__blocked">
          {{ $t('exotic.procBlocked', { n: proc.blockedByAvailability }) }}
        </span>
        <span v-if="proc.error > 0" class="ps-proc__err">✗ {{ proc.error }}</span>
      </div>
    </section>

    <!-- 首次加载占位。 -->
    <div v-if="store.loading.value && rows.length === 0" class="ps-empty">
      <span class="ps-spinner" aria-hidden="true" />
    </div>

    <!-- 空态：无可装、无已装。 -->
    <div v-else-if="rows.length === 0" class="ps-empty">
      <PackageOpen :size="48" />
      <p class="ps-empty__title">{{ $t('exotic.storeEmpty') }}</p>
      <p class="ps-empty__hint">{{ $t('exotic.storeEmptyHint') }}</p>
    </div>

    <!-- 插件列表。 -->
    <div v-else class="ps-list">
      <div v-for="row in rows" :key="row.pluginId" class="ps-card">
        <div class="ps-card__icon" aria-hidden="true"><Puzzle :size="22" /></div>

        <div class="ps-card__body">
          <div class="ps-card__title-row">
            <span class="ps-card__name">{{ row.pluginId }}</span>
            <span class="ps-badge" :class="'ps-badge--' + statusKey(row)">{{
              statusLabel(row)
            }}</span>
          </div>

          <div v-if="row.formats.length" class="ps-card__formats">
            <span v-for="f in row.formats" :key="f" class="ps-chip">{{ f.toUpperCase() }}</span>
          </div>

          <div class="ps-card__meta">
            <span v-if="row.installedVersion">
              {{ $t('exotic.version') }} {{ row.installedVersion }}
              <template v-if="row.upgradable && row.availableVersion">
                → {{ row.availableVersion }}</template
              >
            </span>
            <span v-else-if="row.availableVersion">
              {{ $t('exotic.version') }} {{ row.availableVersion }}
            </span>
            <span v-if="row.sku" class="ps-card__sku"><code>{{ row.sku }}</code></span>
          </div>
        </div>

        <div class="ps-card__actions">
          <span v-if="busy[row.pluginId]" class="ps-busy">
            <RefreshCw :size="14" class="spin-anim" />
          </span>
          <template v-else>
            <!-- 未安装 → 安装（目录过期时禁用）。 -->
            <button
              v-if="row.installState === null"
              class="btn btn-primary btn-sm"
              :disabled="!row.availableVersion || row.registryExpired"
              @click="onInstall(row.pluginId)"
            >
              <Download :size="14" /> {{ $t('exotic.install') }}
            </button>
            <template v-else>
              <!-- 可升级 → 升级（同 install 命令拉更高版本）。 -->
              <button
                v-if="row.upgradable"
                class="btn btn-primary btn-sm"
                :disabled="row.registryExpired"
                @click="onInstall(row.pluginId)"
              >
                <ArrowUpCircle :size="14" /> {{ $t('exotic.upgrade') }}
              </button>
              <!-- 损坏 → 修复。 -->
              <button
                v-if="row.installState === 'broken'"
                class="btn btn-primary btn-sm"
                @click="onRepair(row.pluginId)"
              >
                <Wrench :size="14" /> {{ $t('exotic.repair') }}
              </button>
              <!-- 激活（复用 ExoticActivateDialog）。 -->
              <button class="btn btn-ghost btn-sm" @click="activateTarget = row.pluginId">
                <KeyRound :size="14" /> {{ $t('exotic.activateAction') }}
              </button>
              <!-- 卸载（危险，带确认 + 可选移除授权）。 -->
              <button class="btn btn-ghost btn-sm ps-danger" @click="onUninstall(row.pluginId)">
                <Trash2 :size="14" /> {{ $t('exotic.uninstall') }}
              </button>
            </template>
          </template>
        </div>
      </div>
    </div>

    <!-- 激活对话框：由某行「激活」触发。 -->
    <ExoticActivateDialog
      :open="activateTarget !== null"
      :plugin-id="activateTarget ?? ''"
      :feature-name="activateTarget ?? ''"
      @close="activateTarget = null"
      @activated="onActivated"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted } from 'vue'
import {
  RefreshCw,
  Puzzle,
  Download,
  ArrowUpCircle,
  Wrench,
  KeyRound,
  Trash2,
  PackageOpen,
  Play,
  Pause,
  Square,
} from '@lucide/vue'
import { useI18n } from 'vue-i18n'

import ExoticActivateDialog from '../components/exotic/ExoticActivateDialog.vue'
import { useExoticStore, mergeStorePlugins, type StorePluginRow } from '../composables/useExoticStore'
import { useUiStore } from '../stores/uiStore'
import { useConfirm } from '../composables/useConfirm'
import type { IpcError } from '../utils/ipc'

const { t } = useI18n()
const store = useExoticStore()
const ui = useUiStore()
const { confirm } = useConfirm()

// 合并 registry × installed 为展示行（纯函数，见 useExoticStore）。
const rows = computed<StorePluginRow[]>(() =>
  mergeStorePlugins(store.registry.value, store.installed.value),
)
const anyExpired = computed(() => rows.value.some((r) => r.registryExpired))

// 处理进度（可能为 null=未取到）。
const proc = computed(() => store.status.value)
const procTotal = computed(() => {
  const s = proc.value
  return s ? s.pending + s.processing + s.done + s.error : 0
})
const procDonePct = computed(() =>
  procTotal.value > 0 ? Math.round(((proc.value?.done ?? 0) / procTotal.value) * 100) : 0,
)

// 每插件操作忙态（安装/卸载/修复期间禁用该行按钮并显 spinner）。
const busy = reactive<Record<string, boolean>>({})
const procBusy = ref(false)
const refreshing = ref(false)
const activateTarget = ref<string | null>(null)

onMounted(() => {
  void store.loadAll()
})

// ── 状态标签 ────────────────────────────────────────────────────────────────
function statusKey(r: StorePluginRow): string {
  if (r.installState === 'broken') return 'broken'
  if (r.installState === 'disabled') return 'disabled'
  if (r.upgradable) return 'upgradable'
  if (r.installState) return 'installed'
  return 'installable'
}
function statusLabel(r: StorePluginRow): string {
  return t('exotic.state' + statusKey(r).charAt(0).toUpperCase() + statusKey(r).slice(1))
}

// ── 操作封装：忙态 + 成功/失败 toast（错误取后端稳定 code）─────────────────────
async function run(pluginId: string, fn: () => Promise<void>, okKey: string) {
  if (busy[pluginId]) return
  busy[pluginId] = true
  try {
    await fn()
    ui.addToast('success', t(okKey))
  } catch (e) {
    ui.addToast('error', t('exotic.opFailed', { code: (e as IpcError)?.code ?? 'unknown' }))
  } finally {
    busy[pluginId] = false
  }
}

function onInstall(pluginId: string) {
  void run(pluginId, () => store.install(pluginId), 'exotic.installedOk')
}
function onRepair(pluginId: string) {
  void run(pluginId, () => store.repair(pluginId), 'exotic.repairedOk')
}

async function onUninstall(pluginId: string) {
  const { confirmed, checkboxValue } = await confirm({
    title: t('exotic.uninstallTitle'),
    message: t('exotic.uninstallMsg'),
    confirmText: t('exotic.uninstall'),
    showCheckbox: true,
    checkboxLabel: t('exotic.uninstallRemoveLicense'),
    checkboxValue: false,
  })
  if (!confirmed) return
  void run(pluginId, () => store.uninstall(pluginId, checkboxValue), 'exotic.uninstalledOk')
}

// 激活成功 → 刷新已装/进度（授权态变化可能解阻处理）。
function onActivated() {
  void store.loadInstalled()
  void store.loadStatus()
}

// ── 目录刷新 ────────────────────────────────────────────────────────────────
async function onRefresh() {
  if (refreshing.value) return
  refreshing.value = true
  try {
    const summary = await store.refreshRegistry()
    ui.addToast('success', t('exotic.storeRefreshed', { count: summary.pluginCount }))
  } catch (e) {
    ui.addToast('error', t('exotic.storeRefreshFailed', { code: (e as IpcError)?.code ?? e }))
  } finally {
    refreshing.value = false
  }
}

// ── 处理控制（开始/暂停/停止）────────────────────────────────────────────────
async function ctrl(fn: () => Promise<void>) {
  if (procBusy.value) return
  procBusy.value = true
  try {
    await fn()
  } catch (e) {
    ui.addToast('error', t('exotic.opFailed', { code: (e as IpcError)?.code ?? 'unknown' }))
  } finally {
    procBusy.value = false
  }
}
</script>

<style scoped>
.plugin-store {
  height: 100%;
  overflow-y: auto;
  padding: var(--spacing-lg) var(--spacing-xl);
}

/* ── Header ───────────────────────────────────────────────────────────────── */
.ps-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--spacing-md);
  margin-bottom: var(--spacing-lg);
}
.ps-header__text {
  min-width: 0;
}
.ps-title {
  margin: 0;
  font-size: var(--font-size-xl);
  font-weight: 700;
  color: var(--color-text-primary);
}
.ps-subtitle {
  margin: var(--spacing-xs) 0 0;
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}

.ps-banner {
  margin-bottom: var(--spacing-md);
  padding: var(--spacing-sm) var(--spacing-md);
  border-radius: var(--radius-md);
  font-size: var(--font-size-sm);
  line-height: 1.5;
}
.ps-banner--warn {
  background: color-mix(in srgb, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}

/* ── Processing ───────────────────────────────────────────────────────────── */
.ps-proc {
  margin-bottom: var(--spacing-lg);
  padding: var(--spacing-md);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg-elevated);
}
.ps-proc__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--spacing-sm);
}
.ps-proc__title {
  font-weight: 600;
  color: var(--color-text-primary);
}
.ps-proc__ctrls {
  display: flex;
  gap: var(--spacing-xs);
}
.ps-proc__bar {
  height: 6px;
  border-radius: 3px;
  overflow: hidden;
  background: var(--color-bg-hover);
}
.ps-proc__bar-fill {
  height: 100%;
  background: var(--color-accent);
  transition: width 200ms linear;
}
.ps-proc__meta {
  display: flex;
  gap: var(--spacing-md);
  margin-top: 6px;
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
}
.ps-proc__blocked {
  color: var(--color-warning);
}
.ps-proc__err {
  color: var(--color-error);
}

/* ── Empty / loading ──────────────────────────────────────────────────────── */
.ps-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-2xl);
  color: var(--color-text-tertiary);
  text-align: center;
}
.ps-empty__title {
  font-size: var(--font-size-md);
  font-weight: 600;
  color: var(--color-text-secondary);
  margin: 0;
}
.ps-empty__hint {
  margin: 0;
  font-size: var(--font-size-sm);
}
.ps-spinner {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 3px solid var(--color-border-strong);
  border-top-color: var(--color-accent);
  animation: spin 0.7s linear infinite;
}

/* ── Plugin cards ─────────────────────────────────────────────────────────── */
.ps-list {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}
.ps-card {
  display: flex;
  gap: var(--spacing-md);
  padding: var(--spacing-md);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-bg-surface);
}
.ps-card__icon {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 42px;
  height: 42px;
  border-radius: var(--radius-md);
  background: var(--color-accent-subtle);
  color: var(--color-accent);
}
.ps-card__body {
  flex: 1;
  min-width: 0;
}
.ps-card__title-row {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  flex-wrap: wrap;
}
.ps-card__name {
  font-weight: 600;
  color: var(--color-text-primary);
  word-break: break-all;
}
.ps-card__formats {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 6px;
}
.ps-chip {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.04em;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  background: var(--color-bg-hover);
  color: var(--color-text-secondary);
}
.ps-card__meta {
  display: flex;
  flex-wrap: wrap;
  gap: var(--spacing-md);
  margin-top: 6px;
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
}
.ps-card__sku code {
  font-family: var(--font-mono);
}

.ps-badge {
  font-size: 10px;
  font-weight: 700;
  line-height: 1;
  padding: 3px 7px;
  border-radius: var(--radius-full);
}
.ps-badge--installable {
  background: var(--color-bg-hover);
  color: var(--color-text-secondary);
}
.ps-badge--installed {
  background: color-mix(in srgb, var(--color-success) 18%, transparent);
  color: var(--color-success);
}
.ps-badge--upgradable {
  background: var(--color-accent);
  color: #fff;
}
.ps-badge--disabled {
  background: var(--color-bg-hover);
  color: var(--color-text-tertiary);
}
.ps-badge--broken {
  background: color-mix(in srgb, var(--color-error) 18%, transparent);
  color: var(--color-error);
}

.ps-card__actions {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
}
.btn-sm {
  padding: 4px 10px;
  font-size: var(--font-size-xs);
}
.ps-danger {
  color: var(--color-error);
}
.ps-danger:hover {
  background: color-mix(in srgb, var(--color-error) 12%, transparent);
}
.ps-busy {
  display: inline-flex;
  color: var(--color-text-tertiary);
  padding: 0 var(--spacing-sm);
}

.spin-anim {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
