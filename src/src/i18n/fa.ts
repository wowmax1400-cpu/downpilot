// Persian (فارسی) translations. Mirrors en.ts key-for-key.
// DownPilot is RTL under fa: the I18nProvider sets <html dir="rtl">.
// Missing keys fall back to English, so partial coverage is safe.

import type { Dict } from "./index";

export const fa: Dict = {
  // ---- Menu bar ----
  "menu.file": "پرونده",
  "menu.downloads": "دانلودها",
  "menu.view": "نمایش",
  "menu.tasks": "کارها",
  "menu.tools": "ابزارها",
  "menu.help": "راهنما",

  "menu.file.newDownload": "دانلود جدید…",
  "menu.file.addBatch": "افزودن دانلود دسته‌ای…",
  "menu.file.import": "ورود دانلودها…",
  "menu.file.export": "خروجی گرفتن دانلودها…",
  "menu.file.viewHistory": "مشاهده تاریخچه",
  "menu.file.close": "بستن",

  "menu.downloads.resume": "ادامه",
  "menu.downloads.pause": "توقف موقت",
  "menu.downloads.stop": "توقف",
  "menu.downloads.forceStart": "شروع اجباری",
  "menu.downloads.retry": "تلاش مجدد",
  "menu.downloads.redownload": "دانلود مجدد",
  "menu.downloads.openFile": "باز کردن فایل",
  "menu.downloads.openFolder": "باز کردن پوشه",
  "menu.downloads.copyUrl": "کپی نشانی",
  "menu.downloads.copyPath": "کپی مسیر",
  "menu.downloads.rename": "تغییر نام",
  "menu.downloads.moveToQueue": "انتقال به صف…",
  "menu.downloads.changeCategory": "تغییر دسته…",
  "menu.downloads.properties": "ویژگی‌ها",
  "menu.downloads.delete": "حذف",
  "menu.downloads.deleteWithFile": "حذف + فایل",

  "menu.view.all": "همه دانلودها",
  "menu.view.unfinished": "ناتمام",
  "menu.view.finished": "تمام‌شده",
  "menu.view.paused": "موقتاً متوقف",
  "menu.view.errors": "خطاها",

  "menu.tasks.addUrl": "افزودن نشانی…",
  "menu.tasks.batchDownload": "دانلود دسته‌ای…",
  "menu.tasks.grabber": "جمع‌کننده…",
  "menu.tasks.siteManager": "مدیریت سایت‌ها…",
  "menu.tasks.categories": "دسته‌ها…",
  "menu.tasks.scheduler": "زمان‌بند…",
  "menu.tasks.queueManager": "مدیریت صف‌ها…",
  "menu.tasks.startAll": "شروع همه",
  "menu.tasks.pauseAll": "توقف موقت همه",
  "menu.tasks.stopAll": "توقف همه",
  "menu.tasks.retryAllFailed": "تلاش مجدد همه ناموفق‌ها",
  "menu.tasks.clearFinished": "پاک کردن تمام‌شده‌ها",

  "menu.tools.options": "گزینه‌ها…",
  "menu.tools.browserIntegration": "یکپارچگی مرورگر…",
  "menu.tools.about": "درباره DownPilot",

  "menu.help.shortcuts": "کلیدهای میان‌بر",
  "menu.help.about": "درباره DownPilot",

  "menu.search.placeholder": "جستجوی نام فایل، نشانی، دامنه… (Ctrl+F)",
  "menu.search.clear": "پاک کردن جستجو",

  // ---- Prompts / confirms ----
  "prompt.renameTo": "تغییر نام به:",
  "prompt.moveToQueue": "انتقال به صف (نام):",
  "prompt.changeCategory": "تغییر دسته:",
  "confirm.deleteFile": "و فایل روی دیسک حذف شود؟",
  "confirm.deleteAllCompleted": "همه دانلودهای تمام‌شده حذف شوند؟",

  // ---- Keyboard shortcuts dialog ----
  "shortcuts.title": "کلیدهای میان‌بر DownPilot",
  "shortcuts.newDownload": "دانلود جدید",
  "shortcuts.focusSearch": "فوکوس روی جستجو",
  "shortcuts.selectAll": "انتخاب همه",
  "shortcuts.properties": "ویژگی‌ها",
  "shortcuts.options": "گزینه‌ها",
  "shortcuts.openOrProps": "باز کردن / ویژگی‌ها",
  "shortcuts.removeSelected": "حذف انتخاب‌شده‌ها",
  "shortcuts.rename": "تغییر نام",
  "shortcuts.pauseResume": "توقف موقت / ادامه",
  "shortcuts.doubleClickHint": "اگر تمام شده فایل را باز کن، وگرنه ویژگی‌ها",

  // ---- Toolbar ----
  "toolbar.addUrl": "افزودن نشانی",
  "toolbar.addUrl.title": "افزودن دانلود جدید (Ctrl+N)",
  "toolbar.grabber": "جمع‌کننده",
  "toolbar.resume": "ادامه",
  "toolbar.start": "شروع",
  "toolbar.pause": "توقف موقت",
  "toolbar.stop": "توقف",
  "toolbar.forceStart": "شروع اجباری",
  "toolbar.delete": "حذف",
  "toolbar.stopAll": "توقف همه",
  "toolbar.deleteAll": "حذف همه",
  "toolbar.options": "گزینه‌ها",
  "toolbar.scheduler": "زمان‌بند",
  "toolbar.queues": "صف‌ها",
  "toolbar.bandwidth.unlimited": "نامحدود",
  "toolbar.bandwidth.title": "سقف پهنای باند سراسری (برای همه دانلودها)",

  // ---- Sidebar ----
  "sidebar.section.categories": "دسته‌ها",
  "sidebar.section.grabber": "جمع‌کننده",
  "sidebar.section.queues": "صف‌ها",
  "sidebar.allDownloads": "همه دانلودها",
  "sidebar.unfinished": "ناتمام",
  "sidebar.finished": "تمام‌شده",
  "sidebar.paused": "موقتاً متوقف",
  "sidebar.errors": "خطاها",
  "sidebar.siteGrabber": "جمع‌کننده سایت",
  "sidebar.noQueues": "هنوز صفی نیست",
  "sidebar.queued": "در صف",

  // ---- Status bar ----
  "status.downloads": "دانلود",
  "status.paused": "متوقف",
  "status.active": "فعال",
  "status.remaining": "باقی‌مانده",
  "status.cap": "سقف",
  "status.unlimited": "نامحدود",
  "status.cappedAt": "محدود به {speed}",

  // ---- Drop overlay ----
  "drop.title": "نشانی یا فایل فهرست نشانی را رها کنید",
  "drop.sub1": "رها کردن یک نشانی، پنجره دانلود جدید را باز می‌کند.",
  "drop.sub2": "رها کردن چند نشانی یا فایل .txt، پنجره دسته‌ای را باز می‌کند.",

  // ---- Common ----
  "common.close": "بستن",
  "common.cancel": "انصراف",
  "common.save": "ذخیره",
  "common.edit": "ویرایش",
  "common.delete": "حذف",
  "common.open": "باز کردن",
  "common.builtIn": "داخلی",
  "common.none": "(هیچ)",
  "common.default": "(پیش‌فرض)",
  "common.global": "(سراسری)",
  "common.useDefault": "(پیش‌فرض)",

  // ---- Settings dialog ----
  "settings.title": "گزینه‌ها",
  "settings.search.placeholder": "جستجوی تنظیمات…",
  "settings.noMatch": "تنظیمی یافت نشد",
  "settings.section.general": "عمومی",
  "settings.section.downloads": "دانلودها",
  "settings.section.connections": "اتصال‌ها",
  "settings.section.proxy": "پیشکار (Proxy)",
  "settings.section.browser": "یکپارچگی مرورگر",
  "settings.section.clipboard": "کلیپ‌بورد",
  "settings.section.completion": "کنش‌های پس از اتمام",
  "settings.section.notifications": "اطلاع‌ها",
  "settings.section.categories": "دسته‌ها",
  "settings.section.schedules": "زمان‌بندی‌ها",
  "settings.section.schedulesFull": "زمان‌بندی‌ها و پروفایل‌های پهنای باند",
  "settings.section.capture": "دریافت از مرورگر",
  "settings.section.sites": "قواعد سایت‌ها",
  "settings.section.language": "زبان",

  "settings.general.downloadDir": "پوشه پیش‌فرض دانلود",
  "settings.general.theme": "پوسته",
  "settings.general.accent": "رنگ تأکیدی",

  "settings.downloads.defaultThreads": "اتصال پیش‌فرض برای هر دانلود",
  "settings.downloads.maxThreads": "بیشینه اتصال برای هر دانلود",
  "settings.downloads.maxSimultaneous": "بیشینه دانلود همزمان",
  "settings.downloads.speedLimit": "سقف سرعت سراسری (kbps، ۰=نامحدود)",

  "settings.connections.connTimeout": "زمان انتظار اتصال (میلی‌ثانیه)",
  "settings.connections.readTimeout": "زمان انتظار خواندن (میلی‌ثانیه)",
  "settings.connections.maxRetries": "بیشینه تلاش مجدد",
  "settings.connections.retryDelay": "وقفه بین تلاش‌ها (میلی‌ثانیه)",

  "settings.proxy.url": "نشانی پیشکار (http/https/socks5)",
  "settings.proxy.urlHint": "خالی = بدون پیشکار",

  "settings.browser.autoIntercept": "دریافت خودکار دانلودهای مرورگر",
  "settings.browser.ytdlp": "استفاده از yt-dlp برای پیوندهای رسانه‌ای",

  "settings.clipboard.monitor": "پایش کلیپ‌بورد برای نشانی‌های دانلود",

  "settings.completion.defaultAction": "کنش پیش‌فرض",

  "settings.notifications.scheduleEnabled": "فعال‌سازی بازه‌های زمان‌بندی‌شده",
  "settings.notifications.scheduleStart": "شروع زمان‌بندی (HH:MM)",
  "settings.notifications.scheduleEnd": "پایان زمان‌بندی (HH:MM)",

  // ---- Language switcher ----
  "settings.language.label": "زبان",
  "settings.language.hint": "زبان رابط کاربری. فارسی چیدمان را راست‌به‌چپ می‌کند.",
  "settings.language.en": "English",
  "settings.language.fa": "فارسی",

  // ---- Dialog chrome ----
  "dialog.close": "بستن",
};
