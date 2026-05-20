/**
 * Image compression before sending to LLM.
 * Strategy: resize to ≤1024px, convert to JPEG quality 0.8.
 * Small images (< 500KB) are passed through without re-compression.
 */

const MAX_DIMENSION = 1024
const JPEG_QUALITY = 0.8
const BYPASS_THRESHOLD_BYTES = 500 * 1024 // 500KB

export async function compressImage(file: File): Promise<string> {
  // Small files: just read as data URL
  if (file.size < BYPASS_THRESHOLD_BYTES) {
    return fileToDataUrl(file)
  }

  return new Promise((resolve, reject) => {
    const img = new Image()
    img.onload = () => {
      const { width, height } = calcSize(img.width, img.height, MAX_DIMENSION)

      const canvas = document.createElement('canvas')
      canvas.width = width
      canvas.height = height
      const ctx = canvas.getContext('2d')
      if (!ctx) {
        reject(new Error('Canvas not supported'))
        return
      }
      ctx.drawImage(img, 0, 0, width, height)

      canvas.toBlob(
        (blob) => {
          if (!blob) {
            // fallback: use original
            fileToDataUrl(file).then(resolve).catch(reject)
            return
          }
          const reader = new FileReader()
          reader.onload = () => resolve(reader.result as string)
          reader.onerror = reject
          reader.readAsDataURL(blob)
        },
        'image/jpeg',
        JPEG_QUALITY,
      )
    }
    img.onerror = () => reject(new Error('Image decode failed'))
    img.src = URL.createObjectURL(file)
  })
}

function calcSize(
  w: number,
  h: number,
  max: number,
): { width: number; height: number } {
  if (w <= max && h <= max) return { width: w, height: h }
  const ratio = Math.min(max / w, max / h)
  return { width: Math.round(w * ratio), height: Math.round(h * ratio) }
}

/** Simple File → data URL (no compression) */
function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
}
