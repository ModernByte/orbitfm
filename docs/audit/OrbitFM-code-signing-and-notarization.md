# Подпись и нотаризация релизов OrbitFM (Windows / macOS / Linux)

| Поле | Значение |
|------|----------|
| **Дата обзора** | 2026-11-03 |
| **Проект** | OrbitFM (ModernByte), MIT |
| **Статус** | Справочник; цены и условия меняются — сверять по ссылкам |
| **Цель** | Убрать или смягчить блокировки ОС при установке (SmartScreen, Gatekeeper) |

Документ для владельца/релиз-инженера. Не заменяет юридическую консультацию и не фиксирует ADR.

---

## 1. Что блокирует установку

| Платформа | Механизм | Что видит пользователь без «правильной» подписи |
|-----------|----------|--------------------------------------------------|
| **Windows** | SmartScreen, UAC «Unknown publisher» | Предупреждение, «Запустить всё равно», недоверие к MSI/EXE |
| **macOS** | Gatekeeper + quarantine | «Не удаётся открыть…», нужно ПКМ → Открыть / `xattr` |
| **Linux** | Нет единого SmartScreen | Зависит от канала: AppImage «просто файл»; Flathub/distro — доверие репозитория |

**Self-signed сертификат** (сам себе CA) **не** убирает предупреждения на чужих машинах: система не доверяет вашему корневому ключу.

**Timestamp** при подписи обязателен: после истечения cert старые сборки с RFC-3161 timestamp остаются проверяемыми.

---

## 2. Сравнение вариантов (сводка)

| Вариант | Платформа | Ориентир цены (2026) | Издатель в UI | Убирает предупреждение? | Для OrbitFM |
|---------|-----------|---------------------|---------------|-------------------------|-------------|
| SignPath Foundation (OSS) | Windows (и др. артефакты через SignPath) | **$0** при одобрении | Часто **SignPath Foundation**, не ModernByte | Сильно снижает SmartScreen vs unsigned | **Первый кандидат** |
| Certum Open Source | Windows | от **€49** (cloud) / от **€69** (card set) | «Open Source Developer» + имя | Да, Trusted CA; не для коммерции | Бюджетный бренд-ближе |
| OV Code Signing (SSL.com, Sectigo, DigiCert…) | Windows | примерно **$65–$700+/год** | Ваша орг./имя после валидации | Да; репутация SmartScreen набирается | Свой бренд, платно |
| EV Code Signing | Windows | примерно **$226–$600+/год** | Организация, усиленная проверка | Да; для **kernel drivers** — отдельная ценность | OrbitFM UI app — обычно избыточно |
| Apple Developer Program + Developer ID + Notarize | macOS | **~$99/год** (+ налоги) | Developer ID team | Да, после нотаризации + staple | **Единственный нормальный путь** |
| Ad-hoc `codesign -s -` | macOS | $0 | — | **Нет** для скачанных с интернета | Только свои Mac |
| Flathub / distro packages | Linux | $0 (+ время ревью) | Репозиторий | Иная модель доверия | Linux-first канал |
| AppImage + SHA256SUMS (+ GPG) | Linux | $0 | — | Checksums, не OS SmartScreen | Уже в духе релизов |
| Google Play (Android later) | Android | **$25 один раз** (регистрация) | Store | Store pipeline | Companion, не desktop |

Цены — ориентиры на дату обзора; у реселлеров и в регионах отличаются. Перед покупкой открывать **официальную** витрину CA.

---

## 3. Windows — подробно

### 3.1. Официальная/полезная документация

| Тема | URL |
|------|-----|
| SignPath Foundation (бесплатно для OSS) | https://signpath.org/ |
| Условия OSS SignPath | https://signpath.org/terms.html |
| SignPath OSS product page | https://signpath.io/solutions/open-source-community |
| Документация SignPath (signing, projects) | https://docs.signpath.io/ |
| Microsoft: Authenticode / SignTool | https://learn.microsoft.com/windows/win32/seccrypto/signtool |
| Microsoft: SmartScreen overview | https://learn.microsoft.com/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/ |
| DigiCert Code Signing (сравнение тарифов) | https://www.digicert.com/signing/compare-code-signing-certificates |
| DigiCert Code Signing landing | https://www.digicert.com/signing/code-signing-certificates |
| Certum Open Source (shop) | https://shop.certum.eu/open-source-code-signing.html |
| Certum Code Signing catalog | https://shop.certum.eu/code-signing.html |

### 3.2. SignPath Foundation (рекомендуемый $0-путь для MIT)

**Суть:** для проектов с OSI-лицензией (MIT подходит) Foundation может подписывать артефакты; ключ в **их HSM**, интеграция с CI.

**Условия (сверять `terms.html`):** среди прочего:

- OSI-approved license, **без** commercial dual-licensing всех компонентов;
- не malware / PUP;
- подпись только своих бинарников из своего репо;
- публичная **code signing policy** на сайте проекта;
- MFA у команды; роли committer/reviewer/approver;
- privacy note (или policy).

**Плюсы:** $0; CI-friendly; не нужен личный USB-токен.  
**Минусы:** заявка и ревью; в UI издатель может быть **SignPath Foundation**; жёсткие OSS-условия.

**Действия:** Apply на https://signpath.org/ → указать `https://github.com/ModernByte/orbitfm`, страницу релизов, краткое описание.

### 3.3. Certum Open Source (дешёвый «почти свой» бренд)

По витрине Certum (EU shop, 2026):

| Опция | Ориентир |
|-------|----------|
| Open Source **in the Cloud** (SimplySign) | from **€49** |
| Set: cert + card + reader | from **€69** |
| Activation code (своя карта) | from **€25** |

Ограничение: позиционируется для **некоммерческого** OSS; коммерческая дистрибуция может привести к отзыву (см. условия Certum).

### 3.4. Обычный OV / EV (свой издатель)

**DigiCert** (публичный compare, подписка ~12 мес., USD, без учёта скидок реселлеров):

| Продукт (пример) | Ориентир list |
|------------------|---------------|
| Code Signing Own Token / HSM | ~**$44/мес** → ~**$696/год** |
| Code Signing USB Token | ~**$54/мес** → ~**$840/год** |
| Code Signing + KeyLocker (cloud) | ~**$65/мес** → ~**$996/год** |
| EV-варианты | выше (KeyLocker EV и т.д.) |

На рынке OV часто находят **дешевле** через SSL.com / Certum / реселлеров (ориентиры обзоров 2026: OV от ~$65–$150/год у части продавцов; EV часто $226+). **Всегда** уточнять:

- входит ли токен/HSM/cloud signing;
- лимит подписей/год;
- organization vs individual validation.

**EV** для обычного MSI OrbitFM **не обязателен**. EV критичен скорее для kernel-mode drivers / части enterprise-политик. С 2023+ **и OV, и EV** требуют хранения ключа на токене/HSM (CA/B), не «pfx в git».

### 3.5. Что подписывать у OrbitFM

- `OrbitFM-*-windows-x64.msi` и/или вложенные EXE из staging;
- timestamp server CA (например значение из документации выбранного CA);
- проверка: `signtool verify /pa /v file.msi`.

SmartScreen: даже с валидной подписью **новый** сертификат может предупреждать, пока не накопится репутация загрузок. Unsigned → почти всегда хуже.

---

## 4. macOS — подробно

### 4.1. Официальные ссылки Apple

| Тема | URL |
|------|-----|
| Apple Developer Program | https://developer.apple.com/programs/ |
| Membership details / pricing (ориентир **USD 99**/год) | https://developer.apple.com/support/compare-memberships/ |
| Developer ID | https://developer.apple.com/developer-id/ |
| Notarizing macOS software | https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution |
| Resolving notarization issues | https://developer.apple.com/documentation/security/resolving-common-notarization-issues |
| Hardened Runtime | https://developer.apple.com/documentation/security/hardened-runtime |

### 4.2. Стоимость

| Статья | Ориентир |
|--------|----------|
| Apple Developer Program | **USD 99 / год** (в регионах — местная валюта; + налог) |
| Developer ID certificates | Включены в Program |
| Нотаризация (`notarytool`) | **Без** отдельной платы за билд (есть rate limits) |
| Enterprise Program | **USD 299 / год** — для internal distribution, не для публичного OrbitFM |

Бесплатный Apple ID **не** даёт Developer ID + notarization для раздачи с сайта.

### 4.3. Минимальный pipeline

1. Вступить в Developer Program.  
2. Создать **Developer ID Application** (Account Holder).  
3. Собрать `.app` / `.dmg`.  
4. `codesign --deep --force --options runtime --timestamp --sign "Developer ID Application: …"`.  
5. `xcrun notarytool submit … --wait`.  
6. `xcrun stapler staple OrbitFM.dmg` (или app).  
7. Проверка на чистой машине: скачать по HTTPS, открыть без `xattr` hack.

**Ad-hoc** (`codesign -s -`) и «локальный» Apple Development cert для Gatekeeper при скачивании с GitHub **не** заменяют Developer ID + notarize.

Требования notary (сводно): валидная Developer ID подпись, Hardened Runtime, secure timestamp, SDK ≥ 10.9, корректные entitlements.

---

## 5. Linux

| Канал | Доверие | Ссылки / заметки |
|-------|---------|------------------|
| **Flathub** | Пользователи доверяют репо Flathub | https://docs.flatpak.org/en/latest/publishing.html — ревью, app-id `org.modernbyte.OrbitFM` |
| **AppImage** | Файл + `SHA256SUMS` (+ опционально GPG) | Подпись GPG — ваша идентичность, не Microsoft/Apple store trust |
| **AUR / distro** | Мейнтейнер + репо | Отдельные процессы |

Для Linux-first продукта это **основной** канал без $99 и без OV.

---

## 6. Android (на будущее, companion)

| Статья | Ориентир | URL |
|--------|----------|-----|
| Google Play registration | **$25 один раз** | https://support.google.com/googleplay/android-developer/answer/6112435 |
| Signing App Bundles | Play App Signing рекомендуется | https://developer.android.com/studio/publish/app-signing |

Это не заменяет Windows/macOS desktop signing.

---

## 7. Рекомендуемая стратегия для ModernByte / OrbitFM

### Фаза 0 — сейчас ($0)

1. Linux: AppImage + **SHA256SUMS** на Releases; GPG optional.  
2. README: честные инструкции Windows SmartScreen / macOS Gatekeeper (как есть).  
3. Не класть self-signed «для галочки».

### Фаза 1 — Windows OSS ($0 при успехе)

1. Подготовить **Code signing policy** в репо (короткая markdown + privacy one-liner).  
2. MFA на GitHub org/team.  
3. Заявка **SignPath Foundation**: https://signpath.org/  
4. Встроить submit signing в `release.yml` по docs.signpath.io.  
5. Подписывать MSI/EXE из CI.

### Фаза 2 — macOS (платно, если нужна «Открыть без плясок»)

1. Apple Developer Program (~$99/год).  
2. Developer ID + notarytool + staple в `packaging/macos`.  
3. Обновить README: убрать quarantine-workaround как основной путь.

### Фаза 3 — свой Windows-бренд (если SignPath недостаточен)

1. OV code signing (Certum OSS / SSL.com / иное — сравнить на дату покупки).  
2. Cloud signing или токен + CI runner с доступом.  
3. Timestamp + verify в pipeline.

### Что не делать

- Обещать «никогда не будет SmartScreen» — репутация накапливается.  
- Хранить private key code signing в git/artifacts.  
- Путать App Store iOS ($99) с «бесплатной подписью Windows».  
- Покупать EV только из-за маркетинга, если нет drivers/enterprise requirement.

---

## 8. Чеклист релиза (после подписей)

**Windows**

- [ ] MSI/EXE подписаны  
- [ ] Timestamp OK  
- [ ] `signtool verify` OK  
- [ ] SHA256 в `SHA256SUMS`  
- [ ] Чистая VM: поведение SmartScreen зафиксировано скриншотом для release notes  

**macOS**

- [ ] Developer ID signature (`codesign -dv --verbose=4`)  
- [ ] Notarization accepted  
- [ ] Stapled  
- [ ] Скачивание с GitHub на чистом Mac: открытие без `xattr -c`  

**Linux**

- [ ] SHA256SUMS  
- [ ] (opt) Flathub build  
- [ ] (opt) GPG detach-sign  

---

## 9. Ориентир бюджета на год (грубо)

| Сценарий | Windows | macOS | Итого / год |
|----------|---------|-------|-------------|
| Только Linux + честный README | $0 | $0 | **$0** |
| + SignPath OSS | $0 | $0 | **$0** |
| + Apple notarization | $0 | ~$99 | **~$99** |
| + Certum OSS cloud | ~€49 | ~$99 | **~$150** |
| + DigiCert-class OV list | ~$700 | ~$99 | **~$800** (верхняя оценка list) |

Выбор «SignPath + Apple $99» часто оптимален для OSS с macOS-сборкой.

---

## 10. История документа

| Дата | Изменение |
|------|-----------|
| 2026-11-03 | Первый обзор ссылок, цен-ориентиров, стратегии OrbitFM |

*Перепроверяйте цены и ToS по официальным URL перед оплатой или заявкой. Данные могли измениться после 2026-11-03.*
