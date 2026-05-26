import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import translationEN from './locales/en-US.json'
import translationZH from './locales/zh-CN.json'

// 获取浏览器语言或默认中文
const getDefaultLanguage = (): string => {
  const saved = localStorage.getItem('jeeves-language')
  if (saved) return saved
  const browserLang = navigator.language.toLowerCase()
  if (browserLang.startsWith('zh')) return 'zh-CN'
  if (browserLang.startsWith('ja')) return 'zh-CN' // 暂时日语也用中文，后续完善
  return 'zh-CN'
}

const resources = {
  'en-US': { translation: translationEN },
  'zh-CN': { translation: translationZH }
}

i18n
  .use(initReactI18next)
  .init({
    resources,
    lng: getDefaultLanguage(),
    fallbackLng: 'zh-CN',
    interpolation: {
      escapeValue: false
    },
    detection: {
      order: ['localStorage', 'navigator'],
      caches: ['localStorage'],
      lookupLocalStorage: 'jeeves-language'
    }
  })

export default i18n
