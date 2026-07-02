// src/composables/useAnalysisController.ts
// 分析管理控制器（S6/T19 去重）。aiStore 与 faceStore 的"分析管理半部"此前是显式镜像复制
// （faceStore 注释「Mirrors aiStore's analysis-management half」）：2s 轮询 + start/pause/
// restart/stop + 自动续传 + providerLabel + 进度，逻辑雷同，仅端点 / 状态字段 / 错误处理不同。
//
// 本控制器把这条共享控制流参数化:差异收敛为「静态配置（5 个 IPC 命令 + logTag）+ 进度字段
// getter + 2 个回调（onError / onStarted）」,无运行时耦合、无 state 外漏——故是真 DRY,而非
// 把耦合搬成长参数列表。各 store 只持自己的 status ref 与专属逻辑（搜索 / 模型库等），分析半部
// 委托本控制器。

import { computed, watch, type Ref } from 'vue'
import { invokeIpc, type IpcCommand } from '../utils/ipc'
import i18n from '../i18n'
import { useUiStore } from '../stores/uiStore'

/** 分析状态的公共子集——控制器仅依赖这些字段，各 store 的完整 status 类型须含之。 */
export interface BaseAnalysisStatus {
  provider: string
  isAnalyzing: boolean
  analysisActive: boolean
  totalItems: number
  pendingItems: number
}

/** 该分析种类的 5 个后端命令（AI / face 各一套）。 */
export interface AnalysisCommands {
  getStatus: IpcCommand
  start: IpcCommand
  pause: IpcCommand
  restart: IpcCommand
  stop: IpcCommand
}

export type AnalysisAction = 'start' | 'pause' | 'restart' | 'stop' | 'autoResume'

export interface AnalysisControllerOptions<S extends BaseAnalysisStatus> {
  /** 本 store 持有的分析状态 ref（控制器就地读写其公共字段）。 */
  status: Ref<S>
  commands: AnalysisCommands
  /** 已分析计数 getter——AI 取 analyzedItems、face 取 processedItems（字段名不同，故传 getter）。 */
  analyzedCount: () => number
  /** 日志前缀，如 '[AI]' / '[Face]'。 */
  logTag: string
  /** 动作出错回调——AI 走 console.error；face 的 start/restart 走 toast、pause/stop 走 console。 */
  onError: (action: AnalysisAction, e: unknown) => void
  /** 成功启动 / 重启后的钩子——AI 用它清 searchError；face 无（不传）。 */
  onStarted?: () => void
}

export function useAnalysisController<S extends BaseAnalysisStatus>(
  opts: AnalysisControllerOptions<S>,
) {
  const { status, commands } = opts

  /** 从后端拉最新状态（失败静默——状态栏保留最后已知值）。 */
  async function fetchStatus() {
    try {
      status.value = await invokeIpc<S>(commands.getStatus)
    } catch {
      // Silently ignore — keep last known state | 静默忽略 — 保留最后已知状态
    }
  }

  // 运行中每 2s 轮询状态（isAnalyzing 翻 true 时起、翻 false 时由 onCleanup 清定时器）。
  watch(
    () => status.value.isAnalyzing,
    (isAnalyzing, _, onCleanup) => {
      if (isAnalyzing) {
        const interval = setInterval(() => {
          void fetchStatus()
        }, 2000)
        onCleanup(() => clearInterval(interval))
      }
    },
  )

  // 进度用 analyzedCount / total（face 的 processedItems 含失败项，故失败时进度条仍到 100%）。
  const analyzeProgress = computed(() => {
    if (status.value.totalItems === 0) return 0
    return Math.round((opts.analyzedCount() / status.value.totalItems) * 100)
  })

  const providerLabel = computed(() => {
    const p = status.value.provider
    if (p === 'directml') return 'DirectML'
    if (p === 'cuda') return 'CUDA'
    if (p === 'coreml') return 'CoreML'
    if (p === 'openvino') return 'OpenVINO'
    if (p === 'cpu') return 'CPU'
    // computed 内取 t：Composer 的 locale 是 ref，t() 读它即被追踪 → 切语言时自动重算。
    return i18n.global.t('settings.aiProviderNotInitialized')
  })

  /** 启动前确认有扫描根，否则提示并拦下（AI / face 共用的前置守门）。 */
  async function ensureScanRoots(): Promise<boolean> {
    const { useScanStore } = await import('../stores/scanStore')
    const scan = useScanStore()
    if (!scan.hasScanRoots) {
      useUiStore().addToast('warning', i18n.global.t('common.addScanFolderFirst'))
      return false
    }
    return true
  }

  /** 启动 / 续传分析（不重置——跳过已处理项）。 */
  async function startAnalysis() {
    if (!(await ensureScanRoots())) return
    try {
      await invokeIpc(commands.start)
      status.value.isAnalyzing = true
      status.value.analysisActive = true
      opts.onStarted?.()
    } catch (e) {
      opts.onError('start', e)
    }
  }

  /** 暂停——保留进度与续传标志，释放共享 GPU 槽。 */
  async function pauseAnalysis() {
    try {
      await invokeIpc(commands.pause)
      status.value.isAnalyzing = false
      await fetchStatus()
    } catch (e) {
      opts.onError('pause', e)
    }
  }

  /** 从零重新开始——清空后全量重跑。 */
  async function restartAnalysis() {
    try {
      await invokeIpc(commands.restart)
      status.value.isAnalyzing = true
      status.value.analysisActive = true
      opts.onStarted?.()
    } catch (e) {
      opts.onError('restart', e)
    }
  }

  /** 停止并清除续传标志（不再自动续传）。进度 / 数据保留。 */
  async function stopAnalysis() {
    try {
      await invokeIpc(commands.stop)
      status.value.isAnalyzing = false
      status.value.analysisActive = false
      await fetchStatus()
    } catch (e) {
      opts.onError('stop', e)
    }
  }

  /** 启动时续传被中断（崩溃/强退/暂停）且仍有剩余的分析——断点续传入口。 */
  async function maybeAutoResume() {
    await fetchStatus()
    if (status.value.analysisActive && status.value.pendingItems > 0 && !status.value.isAnalyzing) {
      try {
        await invokeIpc(commands.start)
        status.value.isAnalyzing = true
        console.info(`${opts.logTag} auto-resumed interrupted analysis | 已自动续传被中断的分析`)
      } catch (e) {
        opts.onError('autoResume', e)
      }
    }
  }

  return {
    fetchStatus,
    analyzeProgress,
    providerLabel,
    startAnalysis,
    pauseAnalysis,
    restartAnalysis,
    stopAnalysis,
    maybeAutoResume,
  }
}
