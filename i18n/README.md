# App translations

One dictionary per language, in the site's 13 languages: en es pt de fr it pl tr uk ru id ko zh.

- The key is the Russian text in the code: `tr("Пауза до {time}", …)`.
  `ru.json` holds only the plural forms, every other language holds every key.
- `{name}` placeholders are kept as is.
- A plural key (`trn("{n} человек", n)`) maps to CLDR forms:
  ru/uk/pl `one few many`, id/ko/zh `other`, the rest `one other`.
- Missing text falls back to English, then to the Russian key.
- The language is the one picked in Settings → Account → Language, else the
  system's first language the app has, else English.

Terms follow the site ([site repo](https://github.com/lovearound-me/site), `i18n/*.json`): «Кому-нибудь на свете» = "Someone out there",
«О ком ты сейчас думаешь?» = "Who are you thinking of?", «Рядом» = "Close ones",
«Книга» = "Contacts", «кружки» = "circles". Informal "you" everywhere; "Love ID" is not translated.

    python3 i18n/check.py          # missing, unused and broken entries, per language
    python3 i18n/check.py --keys   # every key and where it is used

The app compiles them in (`include_str!` in `src-tauri/src/i18n.rs`).
