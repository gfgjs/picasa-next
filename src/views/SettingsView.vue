<template>
  <div class="settings-view">
    <header class="settings-header">
      <h1 class="settings-title">{{ $t('settings.title') }}</h1>
      <div class="settings-header__actions">
        <!-- 一键全部折叠/展开：全部展开时收起全部，否则展开全部。 -->
        <button
          class="btn-toggle-all"
          :title="cards.allOpen.value ? $t('settings.collapseAll') : $t('settings.expandAll')"
          @click="toggleAllCards"
        >
          <component :is="cards.allOpen.value ? ChevronsDownUp : ChevronsUpDown" :size="15" />
          <span>{{
            cards.allOpen.value ? $t('settings.collapseAll') : $t('settings.expandAll')
          }}</span>
        </button>
        <button
          class="btn-close"
          :title="$t('settings.closeSettings')"
          :aria-label="$t('settings.closeSettings')"
          @click="closeSettings"
        >
          <X :size="18" />
        </button>
      </div>
    </header>

    <main class="settings-content">
      <!-- ── 外观 ─────────────────────────────────────────── -->
      <CollapsibleCard id="general" :title="$t('settings.general')">
        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('theme') }"
            @click="ui.togglePinnedSetting('theme')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.theme') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.themeDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="theme" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('language') }"
            @click="ui.togglePinnedSetting('language')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.language') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.languageDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="language" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('uiFontSize') }"
            @click="ui.togglePinnedSetting('uiFontSize')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.uiFontSize') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.uiFontSizeDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="uiFontSize" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('hoverScale') }"
            @click="ui.togglePinnedSetting('hoverScale')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.hoverScale') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.hoverScaleDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="hoverScale" />
        </div>
        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('hoverAutoplay') }"
            @click="ui.togglePinnedSetting('hoverAutoplay')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.hoverAutoplay') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.hoverAutoplayDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="hoverAutoplay" />
        </div>
        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('closeBehavior') }"
            @click="ui.togglePinnedSetting('closeBehavior')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.closeBehavior') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.closeBehaviorDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="closeBehavior" />
        </div>
      </CollapsibleCard>

      <!-- ── 缩略图 ───────────────────────────────────────── -->
      <CollapsibleCard id="thumbnails" :title="$t('settings.thumbnails')">
        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('showThumbInfo') }"
            @click="ui.togglePinnedSetting('showThumbInfo')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.thumbInfoHover') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.thumbInfoHoverDesc') }}</div>
          </div>
          <div
            style="
              display: flex;
              flex-direction: column;
              gap: 8px;
              align-items: flex-end;
              max-width: 65%;
            "
          >
            <DynamicSettingControl setting-key="showThumbInfo" />
            <div
              v-if="ui.showThumbInfo"
              style="
                display: flex;
                gap: 12px;
                font-size: 13px;
                align-items: center;
                flex-wrap: wrap;
                justify-content: flex-end;
                margin-top: 8px;
                color: var(--color-text-primary);
              "
            >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('status')"
                  @click="handleThumbInfoToggle($event, 'status')"
                />{{ $t('settings.thumbInfoStatus') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('favorite')"
                  @click="handleThumbInfoToggle($event, 'favorite')"
                />{{ $t('settings.thumbInfoFavorite') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('size')"
                  @click="handleThumbInfoToggle($event, 'size')"
                />{{ $t('settings.thumbInfoSize') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('resolution')"
                  @click="handleThumbInfoToggle($event, 'resolution')"
                />{{ $t('settings.thumbInfoResolution') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('date')"
                  @click="handleThumbInfoToggle($event, 'date')"
                />{{ $t('settings.thumbInfoDate') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('filename')"
                  @click="handleThumbInfoToggle($event, 'filename')"
                />{{ $t('settings.thumbInfoFilename') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('path')"
                  @click="handleThumbInfoToggle($event, 'path')"
                />{{ $t('settings.thumbInfoPath') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('geo')"
                  @click="handleThumbInfoToggle($event, 'geo')"
                />{{ $t('settings.thumbInfoLocation') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('camera')"
                  @click="handleThumbInfoToggle($event, 'camera')"
                />{{ $t('settings.thumbInfoCamera') }}</label
              >
              <label style="display: flex; align-items: center; gap: 4px"
                ><input
                  type="checkbox"
                  :checked="ui.thumbInfoElements.includes('params')"
                  @click="handleThumbInfoToggle($event, 'params')"
                />{{ $t('settings.thumbInfoParams') }}</label
              >
            </div>
          </div>
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('thumbDecodeStrategy') }"
            @click="ui.togglePinnedSetting('thumbDecodeStrategy')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.thumbDecodeStrategy') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.thumbDecodeDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="thumbDecodeStrategy" />
        </div>

        <div class="settings-card__item" v-if="config.thumbStrategy === 'gpu'">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('gpuEngine') }"
            @click="ui.togglePinnedSetting('gpuEngine')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.gpuEngine') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.gpuEngineDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="gpuEngine" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('thumbCacheDir') }"
            @click="ui.togglePinnedSetting('thumbCacheDir')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.thumbCacheDir') }}
            </div>
            <div
              class="settings-card__desc clickable-path"
              @click="openDirectory(thumbCacheDir)"
              :title="$t('settings.openInExplorer')"
            >
              {{ thumbCacheDir || $t('settings.fetchingPath') }}
            </div>
          </div>
          <button class="btn btn-secondary" @click="changeCacheDir">
            {{ $t('settings.changeDir') }}
          </button>
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('thumbSize') }"
            @click="ui.togglePinnedSetting('thumbSize')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.thumbSize') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.thumbSizeHint') }}</div>
          </div>
          <DynamicSettingControl setting-key="thumbSize" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('thumbSkipMaxKb') }"
            @click="ui.togglePinnedSetting('thumbSkipMaxKb')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.thumbSkipMaxKb') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.thumbSkipDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="thumbSkipMaxKb" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('thumbCacheMaxMb') }"
            @click="ui.togglePinnedSetting('thumbCacheMaxMb')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.thumbCacheMaxMb') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.thumbCacheDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="thumbCacheMaxMb" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('timelineScrollWidth') }"
            @click="ui.togglePinnedSetting('timelineScrollWidth')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.timelineScrollWidth') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.timelineScrollDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="timelineScrollWidth" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('fullThumbGen') }"
            @click="ui.togglePinnedSetting('fullThumbGen')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.fullThumbGen') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.fullThumbGenDesc') }}</div>
            <div v-if="scan.thumbGenProgress.status !== 'idle'" class="thumb-gen-status">
              <div class="progress-bar">
                <div
                  class="progress-bar__fill"
                  :class="{ 'progress-shimmer': scan.thumbGenProgress.isRunning }"
                  :style="{ width: thumbGenPercent + '%' }"
                />
              </div>
              <div class="thumb-gen-text" style="display: flex; align-items: center; gap: 6px">
                <span v-if="scan.thumbGenProgress.isRunning">{{
                  $t('settings.genStatusRunning', {
                    generated: scan.thumbGenProgress.generated,
                    total: scan.thumbGenProgress.total,
                  })
                }}</span>
                <span
                  v-if="scan.thumbGenProgress.isRunning && scan.thumbGenProgress.phase"
                  style="
                    font-weight: 500;
                    color: var(--color-warning);
                    border: 1px solid currentColor;
                    padding: 0 4px;
                    border-radius: 4px;
                    font-size: 10px;
                    line-height: 1.2;
                  "
                  >[{{ scan.thumbGenProgress.phase }}]</span
                >
                <span v-else-if="scan.thumbGenProgress.status === 'completed'">{{
                  $t('settings.genStatusCompleted')
                }}</span>
                <span v-else-if="scan.thumbGenProgress.status === 'cancelled'">{{
                  $t('settings.genStatusCancelled')
                }}</span>
                <span v-else-if="scan.thumbGenProgress.status === 'error'">{{
                  $t('settings.genStatusError')
                }}</span>
              </div>
            </div>
          </div>
          <div class="setting-actions">
            <button
              v-if="scan.thumbGenProgress.isRunning"
              class="btn btn-secondary"
              @click="scan.stopFullThumbnailGeneration()"
            >
              {{ $t('settings.stopGen') }}
            </button>
            <button v-else class="btn btn-primary" @click="scan.startFullThumbnailGeneration()">
              {{ $t('settings.startGen') }}
            </button>
          </div>
        </div>
      </CollapsibleCard>

      <!-- ── 视频 ─────────────────────────────────────────── -->
      <CollapsibleCard id="video" :title="$t('settings.video')">
        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('enableVideoCover') }"
            @click="ui.togglePinnedSetting('enableVideoCover')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.enableVideoCover') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.enableVideoCoverDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="enableVideoCover" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('enableVideoKeyframes') }"
            @click="ui.togglePinnedSetting('enableVideoKeyframes')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.enableVideoKeyframes') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.enableVideoKeyframesDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="enableVideoKeyframes" />
        </div>
      </CollapsibleCard>

      <!-- ── AI 模型配置 ──────────────────────────────────── -->
      <CollapsibleCard id="aiModels" :title="$t('settings.aiModels')">
        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('aiEngineStatus') }"
            @click="ui.togglePinnedSetting('aiEngineStatus')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.aiEngineStatus') }}
            </div>
            <div class="settings-card__desc">
              {{ ai.providerLabel }} {{ ai.status.gpuName ? `(${ai.status.gpuName})` : '' }}
              <span v-if="ai.status.vramGb !== null">
                [{{ $t('settings.aiVram') }}: {{ ai.status.vramGb }}GB]</span
              >
              <span v-if="!ai.status.clipLoaded" style="color: var(--color-warning)">
                {{ $t('settings.aiModelNotLoaded') }}</span
              >
              <span v-else style="color: var(--color-success)">
                {{ $t('settings.aiModelLoaded') }}</span
              >
            </div>
          </div>
          <button class="btn btn-secondary" @click="ai.initEngine" :disabled="ai.status.clipLoaded">
            {{ $t('settings.aiTestLoad') }}
          </button>
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('aiHqCache') }"
            @click="ui.togglePinnedSetting('aiHqCache')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.aiHqCache') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.aiHqCacheDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="aiHqCache" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('aiBatchSize') }"
            @click="ui.togglePinnedSetting('aiBatchSize')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.aiBatchSize') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.aiBatchSizeDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="aiBatchSize" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('aiHardwareStrategy') }"
            @click="ui.togglePinnedSetting('aiHardwareStrategy')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.aiHardwareStrategy') }}
            </div>
            <div class="settings-card__desc">
              {{ $t('settings.aiHardwareDesc') }}
            </div>
          </div>
          <DynamicSettingControl setting-key="aiHardwareStrategy" />
        </div>
        <!-- 手动导入 / 图像·文本模型选择已移除：模型的下载与切换统一由下方「模型库」管理。 -->
      </CollapsibleCard>

      <!-- ── AI 模型库（下载与切换，Layer B）──────────────────── -->
      <ModelLibrary />

      <!-- ── 人脸模型库（F7，只读：双轨 + 安装状态）──────────────── -->
      <FaceModelLibrary />

      <!-- ── 网络存储（需求8 8B, §3.8）─────────────────────── -->
      <NetworkStorageSection />

      <!-- ── 已知卷（T13 §3.7 离线 UX）──────────────────────── -->
      <KnownVolumesSection />

      <!-- ── 开发者工具 ─────────────────────────────────── -->
      <CollapsibleCard id="debug" :title="$t('sidebar.debugSettings')">
        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('clearDb') }"
            @click="ui.togglePinnedSetting('clearDb')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.clearDb') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.clearDbDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="clearDb" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('clearSettings') }"
            @click="ui.togglePinnedSetting('clearSettings')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.clearSettings') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.clearSettingsDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="clearSettings" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('logLevel') }"
            @click="ui.togglePinnedSetting('logLevel')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.logLevel') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.logLevelDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="logLevel" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('logDir') }"
            @click="ui.togglePinnedSetting('logDir')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.logDir') }}
            </div>
            <div
              class="settings-card__desc clickable-path"
              @click="openDirectory(logDir)"
              :title="$t('settings.openInExplorer')"
            >
              {{ logDir || $t('settings.fetchingPath') }}
            </div>
          </div>
          <button class="btn btn-secondary" @click="changeLogDir">
            {{ $t('settings.changeDir') }}
          </button>
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('clearAllThumbnails') }"
            @click="ui.togglePinnedSetting('clearAllThumbnails')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.clearAllThumbnails') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.clearAllThumbnailsDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="clearAllThumbnails" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('clearBrowserCache') }"
            @click="ui.togglePinnedSetting('clearBrowserCache')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.clearBrowserCache') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.clearBrowserCacheDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="clearBrowserCache" />
        </div>

        <div class="settings-card__item">
          <button
            class="pin-btn"
            :class="{ active: ui.pinnedSettings.includes('clearLogs') }"
            @click="ui.togglePinnedSetting('clearLogs')"
            :title="$t('settings.pinToSidebar')"
            :aria-label="$t('settings.pinToSidebar')"
          >
            <Pin :size="14" />
          </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px">
              {{ $t('settings.clearLogs') }}
            </div>
            <div class="settings-card__desc">{{ $t('settings.clearLogsDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="clearLogs" />
        </div>
      </CollapsibleCard>
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed, onUnmounted } from 'vue'
import { invokeIpc } from '../utils/ipc'
import { useUiStore } from '../stores/uiStore'
import { useScanStore } from '../stores/scanStore'
import { useMediaStore } from '../stores/mediaStore'
import { useAiStore } from '../stores/aiStore'
import { useConfigStore } from '../stores/configStore'
import { useI18n } from 'vue-i18n'
import {
  X,
  Pin,
  ChevronsDownUp,
  ChevronsUpDown,
} from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { IPC } from '../constants/ipc'
import DynamicSettingControl from '../components/settings/DynamicSettingControl.vue'
import NetworkStorageSection from '../components/settings/NetworkStorageSection.vue'
import KnownVolumesSection from '../components/settings/KnownVolumesSection.vue'
import ModelLibrary from '../components/settings/ModelLibrary.vue'
import FaceModelLibrary from '../components/settings/FaceModelLibrary.vue'
import CollapsibleCard from '../components/settings/CollapsibleCard.vue'
import { useSettingsCards } from '../composables/useSettingsCards'

const ui = useUiStore()
const scan = useScanStore()
const media = useMediaStore()
const ai = useAiStore()
const config = useConfigStore()
const { t } = useI18n()

// 设置卡片折叠协调器：驱动头部「一键全部折叠/展开」。
const cards = useSettingsCards()
function toggleAllCards() {
  cards.setAll(!cards.allOpen.value)
}

const thumbCacheDir = ref('')
const logDir = ref('')

const thumbGenPercent = computed(() => {
  const { generated, total } = scan.thumbGenProgress
  if (!total) return 0
  return Math.min(100, Math.round((generated / total) * 100))
})

onMounted(async () => {
  await config.loadConfig()

  try {
    thumbCacheDir.value = await invokeIpc<string>(IPC.GET_THUMB_CACHE_DIR)
  } catch (e) {
    console.warn('Failed to fetch resolved cache dir', e)
  }

  try {
    logDir.value = await invokeIpc<string>(IPC.GET_LOG_DIR)
  } catch (e) {
    console.warn('Failed to fetch resolved log dir', e)
  }

  try {
    await ai.fetchStatus()
  } catch (e) {
    console.error('Failed to get ai status:', e)
  }

  document.addEventListener('keydown', onKeyDown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', onKeyDown)
})

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') ui.isSettingsOpen = false
}

async function changeLogDir() {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t('settings.chooseLogDir'),
    })
    if (selected && typeof selected === 'string') {
      await config.saveConfig('log_dir', selected)
      logDir.value = selected
      ui.addToast('success', t('settings.logDirChanged'))
    }
  } catch (e) {
    console.error('Failed to select log directory:', e)
  }
}

async function changeCacheDir() {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t('settings.chooseCacheDir'),
    })
    if (selected && typeof selected === 'string') {
      await config.saveConfig('thumb_cache_dir', selected)
      thumbCacheDir.value = selected
      ui.addToast('success', t('settings.cacheDirChanged'))
    }
  } catch (e) {
    console.error('Failed to select directory:', e)
  }
}

async function openDirectory(path: string) {
  if (!path) return
  try {
    await invokeIpc(IPC.OPEN_DIRECTORY, { path })
  } catch (e) {
    ui.addToast('error', t('settings.openDirFailed', { error: e }))
  }
}

function handleThumbInfoToggle(e: MouseEvent, val: string) {
  const el = e.target as HTMLInputElement
  const isChecked = el.checked
  const isAdvanced = ['geo', 'camera', 'params'].includes(val)

  if (isChecked && isAdvanced) {
    if (!window.confirm(t('settings.advancedMetadataWarning'))) {
      e.preventDefault()
      return
    }
  }

  const current = new Set(ui.thumbInfoElements)
  if (isChecked) {
    current.add(val)
  } else {
    current.delete(val)
  }
  ui.setThumbInfoElements(Array.from(current))
  media.invalidateLayout()
}

function closeSettings() {
  ui.isSettingsOpen = false
}
</script>

<style scoped>
.settings-view {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  background: var(--color-bg-primary);
  overflow-y: auto;
}

.settings-header {
  position: sticky;
  top: 0;
  z-index: 10;
  background: color-mix(in srgb, var(--color-bg-primary) 85%, transparent);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--spacing-lg) var(--spacing-xl);
  border-bottom: 1px solid var(--color-border);
  flex-shrink: 0;
}

.settings-title {
  font-size: var(--font-size-xl);
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0;
}

.settings-header__actions {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
}

/* 一键全部折叠/展开 */
.btn-toggle-all {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: var(--radius-md);
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition:
    background var(--transition-fast),
    color var(--transition-fast),
    border-color var(--transition-fast);
}
.btn-toggle-all:hover {
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
  border-color: var(--color-text-tertiary);
}

.btn-close {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  color: var(--color-text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    background var(--transition-fast),
    color var(--transition-fast),
    border-color var(--transition-fast);
}
.btn-close:hover {
  background: var(--color-error);
  color: white;
  border-color: var(--color-error);
}

.settings-content {
  padding: var(--spacing-lg) var(--spacing-xl);
  max-width: 860px;
  margin: 0 auto;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.settings-card__item {
  padding-left: 12px;
}

/* ── Card overrides (extend global .settings-card) ─────────────────────── */
/* The base .settings-card styles are in index.css. */
/* Here we only add component-specific refinements. */

.clickable-path {
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 2px;
  text-decoration-color: transparent;
  transition:
    text-decoration-color var(--transition-fast),
    color var(--transition-fast);
}
.clickable-path:hover {
  color: var(--color-accent);
  text-decoration-color: var(--color-accent);
}

/* ── Thumbnail generation ──────────────────────────────────────────────── */
.thumb-gen-status {
  margin-top: var(--spacing-sm);
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.progress-bar {
  width: 100%;
  height: 4px;
  border-radius: 2px;
  background: var(--color-border);
  overflow: hidden;
}
.progress-bar__fill {
  height: 100%;
  background: var(--color-accent);
  transition: width 100ms linear;
}
.progress-shimmer {
  background: linear-gradient(
    90deg,
    var(--color-accent) 0%,
    var(--color-accent-hover) 50%,
    var(--color-accent) 100%
  );
  background-size: 200% 100%;
  animation: shimmer 1.5s ease-in-out infinite;
}
.thumb-gen-text {
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
@keyframes shimmer {
  0% {
    background-position: -200% 0;
  }
  100% {
    background-position: 200% 0;
  }
}

/* ── Buttons ───────────────────────────────────────────────────────────── */
.setting-actions {
  display: flex;
  gap: var(--spacing-sm);
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
  color: var(--color-primary);
  background: color-mix(in srgb, var(--color-primary) 10%, transparent);
}

.btn {
  padding: 7px 16px;
  border-radius: var(--radius-md);
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  border: none;
  display: inline-flex;
  align-items: center;
  gap: var(--spacing-xs);
  transition:
    background var(--transition-fast),
    filter var(--transition-fast);
  white-space: nowrap;
}
.btn-secondary {
  background: transparent;
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border);
}
.btn-secondary:hover {
  background: var(--color-bg-hover);
}
.btn-primary {
  background: var(--color-accent);
  color: #fff;
}
.btn-primary:hover {
  filter: brightness(1.1);
}
.btn-danger {
  background: transparent;
  color: var(--color-error);
  border: 1px solid var(--color-border);
}
.btn-danger:hover {
  background: var(--color-error);
  color: #fff;
  border-color: var(--color-error);
}
</style>
