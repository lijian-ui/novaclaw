/**
 * 图标生成脚本 — 使用 sharp 库
 * 先执行: npm install --no-save sharp png-to-ico
 * 再执行: node scripts/generate-icons.cjs
 */
const sharp = require('sharp')
const { mkdirSync, copyFileSync, existsSync, unlinkSync, writeFileSync } = require('fs')
const { join, dirname } = require('path')

const ROOT = join(__dirname, '..')
const SRC_PNG = join(ROOT, 'ICONS', 'jeeves.png')
const ICONS_DIR = join(ROOT, 'src-tauri', 'icons')
const PUBLIC_DIR = join(ROOT, 'public')

if (!existsSync(SRC_PNG)) {
  console.error('错误: 找不到源图标文件', SRC_PNG)
  process.exit(1)
}

function resize(input, output, size) {
  const d = dirname(output)
  if (!existsSync(d)) mkdirSync(d, { recursive: true })
  return sharp(input).resize(size, size).toFile(output).then(() => {
    const name = output.split('\\').pop().split('/').pop()
    console.log('  OK', name, '(' + size + 'x' + size + ')')
  })
}

const sizes = [
  { f: '32x32.png', s: 32 },
  { f: '128x128.png', s: 128 },
  { f: '128x128@2x.png', s: 256 },
  { f: 'Square30x30Logo.png', s: 30 },
  { f: 'Square44x44Logo.png', s: 44 },
  { f: 'Square71x71Logo.png', s: 71 },
  { f: 'Square89x89Logo.png', s: 89 },
  { f: 'Square107x107Logo.png', s: 107 },
  { f: 'Square142x142Logo.png', s: 142 },
  { f: 'Square150x150Logo.png', s: 150 },
  { f: 'Square284x284Logo.png', s: 284 },
  { f: 'Square310x310Logo.png', s: 310 },
  { f: 'StoreLogo.png', s: 50 },
  { f: 'ios/AppIcon-20x20@1x.png', s: 20 },
  { f: 'ios/AppIcon-20x20@2x.png', s: 40 },
  { f: 'ios/AppIcon-20x20@3x.png', s: 60 },
  { f: 'ios/AppIcon-29x29@1x.png', s: 29 },
  { f: 'ios/AppIcon-29x29@2x.png', s: 58 },
  { f: 'ios/AppIcon-29x29@3x.png', s: 87 },
  { f: 'ios/AppIcon-40x40@1x.png', s: 40 },
  { f: 'ios/AppIcon-40x40@2x.png', s: 80 },
  { f: 'ios/AppIcon-40x40@3x.png', s: 120 },
  { f: 'ios/AppIcon-60x60@2x.png', s: 120 },
  { f: 'ios/AppIcon-60x60@3x.png', s: 180 },
  { f: 'ios/AppIcon-76x76@1x.png', s: 76 },
  { f: 'ios/AppIcon-76x76@2x.png', s: 152 },
  { f: 'ios/AppIcon-83.5x83.5@2x.png', s: 167 },
  { f: 'ios/AppIcon-512@2x.png', s: 1024 },
  { f: 'android/mipmap-mdpi/ic_launcher.png', s: 48 },
  { f: 'android/mipmap-mdpi/ic_launcher_foreground.png', s: 108 },
  { f: 'android/mipmap-mdpi/ic_launcher_round.png', s: 48 },
  { f: 'android/mipmap-hdpi/ic_launcher.png', s: 72 },
  { f: 'android/mipmap-hdpi/ic_launcher_foreground.png', s: 162 },
  { f: 'android/mipmap-hdpi/ic_launcher_round.png', s: 72 },
  { f: 'android/mipmap-xhdpi/ic_launcher.png', s: 96 },
  { f: 'android/mipmap-xhdpi/ic_launcher_foreground.png', s: 216 },
  { f: 'android/mipmap-xhdpi/ic_launcher_round.png', s: 96 },
  { f: 'android/mipmap-xxhdpi/ic_launcher.png', s: 144 },
  { f: 'android/mipmap-xxhdpi/ic_launcher_foreground.png', s: 324 },
  { f: 'android/mipmap-xxhdpi/ic_launcher_round.png', s: 144 },
  { f: 'android/mipmap-xxxhdpi/ic_launcher.png', s: 192 },
  { f: 'android/mipmap-xxxhdpi/ic_launcher_foreground.png', s: 432 },
  { f: 'android/mipmap-xxxhdpi/ic_launcher_round.png', s: 192 },
]

async function main() {
  console.log('正在生成图标...\n')

  const base = join(ICONS_DIR, '__base_1024.png')
  await resize(SRC_PNG, base, 1024)

  for (const { f, s } of sizes) {
    await resize(SRC_PNG, join(ICONS_DIR, f), s)
  }

  copyFileSync(SRC_PNG, join(ICONS_DIR, 'icon.png'))
  console.log('  OK  icon.png (原始尺寸)')

  // ICO
  console.log('\n正在生成 Windows ICO...')
  try {
    const { default: pngToIco } = require('png-to-ico')
    const buf = await pngToIco([join(ICONS_DIR, '128x128@2x.png')])
    writeFileSync(join(ICONS_DIR, 'icon.ico'), buf)
    console.log('  OK  icon.ico')
  } catch (e) {
    console.error('  FAIL icon.ico:', e.message)
  }

  // ICNS — 在 Windows 上无法生成，macOS 构建时 Tauri 会自动转换
  console.log('\n正在处理 macOS ICNS...')
  console.log('  SKIP 在 macOS 上构建时由 Tauri 自动从 icon.png 转换')

  try { unlinkSync(base) } catch {}

  // Web favicon
  console.log('\n正在生成 Web favicon...')
  await resize(SRC_PNG, join(PUBLIC_DIR, 'favicon.ico'), 32)
  copyFileSync(SRC_PNG, join(PUBLIC_DIR, 'jeeves.png'))
  console.log('  OK  public/jeeves.png')

  // src/assets/app-icon.png
  copyFileSync(SRC_PNG, join(ROOT, 'src', 'assets', 'app-icon.png'))
  console.log('  OK  src/assets/app-icon.png')

  console.log('\n所有图标生成完成!')
}

main().catch(e => { console.error(e); process.exit(1) })