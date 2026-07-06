import pluginVue from 'eslint-plugin-vue'
import { defineConfigWithVueTs, vueTsConfigs } from '@vue/eslint-config-typescript'
import skipFormatting from '@vue/eslint-config-prettier/skip-formatting'

// Vue 3 + TypeScript strict 官方推荐 flat config（create-vue 同款）。
// - flat/essential：Vue 模板/SFC 必备规则。
// - vueTsConfigs.recommended：typescript-eslint 推荐集（含类型感知规则）。
// - skipFormatting：关闭与 Prettier 冲突的格式类规则，把「格式」交给 Prettier，
//   ESLint 只管代码质量（避免二者打架）。
export default defineConfigWithVueTs(
  {
    name: 'app/files-to-lint',
    files: ['**/*.{ts,mts,tsx,vue}'],
  },
  {
    // Rust（src-tauri）、Python 虚拟环境（venv，含 torch/onnxmltools 打包 JS）、
    // 构建产物、压缩/生成文件不参与前端 lint —— 否则 ESLint 默认会扫到这些 .js/.mjs 噪声。
    name: 'app/files-to-ignore',
    ignores: [
      '**/dist/**',
      '**/dist-ssr/**',
      '**/coverage/**',
      'src-tauri/**',
      // workspace 化后 Rust target/ 在仓库根(tauri codegen 产出 .js/.mjs 资产,非前端源码)。
      'target/**',
      'crates/**',
      'venv/**',
      '**/site-packages/**',
      '**/*.min.js',
    ],
  },
  pluginVue.configs['flat/essential'],
  vueTsConfigs.recommended,
  {
    // R1-7 i18n 防回潮：模板内禁止裸文本 —— 一切用户可见文案必须走 t()/$t()。
    // 规则只查模板文本节点与 title/aria-label/placeholder/alt 等展示型属性；
    // allowlist 准入标准：纯符号 / 计量单位 / 品牌与协议名 / 技术徽标 / 技术占位示例 /
    // 语言自名（endonym，语言选项按惯例以其自身语言显示）——勿放行任何自然语言句子。
    name: 'app/i18n-no-bare-strings',
    files: ['src/**/*.vue'],
    rules: {
      'vue/no-bare-strings-in-template': [
        'error',
        {
          allowlist: [
            // 符号与标点
            '(', ')', ',', '.', '&', '+', '-', '=', '*', '/', '#', '%', '!', '?', ':',
            '[', ']', '{', '}', '<', '>', '·', '•', '‐', '–', '—',
            '−', '|', '×', '…', '‹', '›', '→', '↑', '↓', '✗',
            // 计量单位（紧跟插值的后缀片段）
            'px', 's', 'MB', 'GB',
            // 品牌 / 协议 / 技术徽标
            'Scrollery', 'v0.1.0', 'WebDAV', 'LIVE', 'Live', 'ORIG', 'THUMB', 'fp16',
            '1:1', 'API Key',
            // 技术占位示例（placeholder 中的 URL / 模型名 / 密钥格式）
            'https://api.openai.com/v1', 'https://dav.example.com/remote.php/dav/files/me',
            'gpt-4o-mini', 'sk-...', 'photos',
            // 语言自名（endonym）
            '简体中文', 'English',
          ],
        },
      ],
    },
  },
  {
    // 标准约定：以 `_` 开头的未用变量/参数/解构/catch 视为「有意保留」（接口要求但用不到
    // 的形参、占位解构等），不报 no-unused-vars —— 避免为消警而强删签名必需的参数。
    name: 'app/unused-underscore',
    rules: {
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
          destructuredArrayIgnorePattern: '^_',
        },
      ],
    },
  },
  skipFormatting,
)
