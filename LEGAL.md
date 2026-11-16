# OrbitFM — legal information and disclaimer

**English** · [Русский](#русский)

This is not legal advice: if anything below is unclear in your jurisdiction,
consult your own lawyer.

## English

**(a) AS IS.** OrbitFM is provided under the MIT License, and its “DISCLAIMER OF
WARRANTY” section applies in full: the software is provided “AS IS”, WITHOUT
WARRANTY OF ANY KIND, express or implied, including the warranties of
merchantability, fitness for a particular purpose and noninfringement. We do not
warrant that the program is error-free, that a chosen station will play, or that
a `0.x` release will behave like a later one.

**(b) Liability.** To the extent permitted by applicable law, the authors,
contributors and ModernByte shall not be liable for any direct, indirect,
incidental, special, exemplary or consequential damages, loss of profits, loss of
data, interruption of operation or damage to equipment arising from use of or
inability to use the software. Some jurisdictions do not allow such limitations,
so parts of it may not apply to you; in the United States a disclaimer must be
conspicuous (see
[FTC guidance](https://www.ftc.gov/business-guidance/resources/using-conspicuous-language-make-effective-online-redisclaimers)).

**(c) Stations and streams.** The application does not broadcast, record or store
audio. Station lists come from the third-party
[Radio Browser](https://www.radio-browser.info) directory, maintained by its own
community; we neither moderate it nor vouch for its contents. You are responsible
for the rights in a station's content, for the legality of listening to it in
your jurisdiction and for complying with any service's terms of use.

**(d) Network and personal data.** The application contacts only Radio Browser
servers (mirrors resolved via `all.api.radio-browser.info`, including the listen
notification endpoint `/json/url/<uuid>`) and the addresses you choose: a station
stream and its favicon. Your IP address is visible to the broadcaster, as with
any network request. There are no accounts, no cloud sync and no telemetry, and
settings, favorites and history stay in your own user directories. The network
profile changes the volume and frequency of background requests, but not their
destinations or the URL-checking policy. Adding telemetry would require a
separate architectural decision and explicit opt-in by the user.

**(e) Trademarks.** Noctalia, niri, Hyprland, mpv, Radio Browser, Rust, Windows,
macOS and other names and logos belong to their owners; mentioning them implies
no affiliation, endorsement or sponsorship.

**(f) Pre-release status.** Until 1.0.0 the application ships as a `0.x`
pre-release, and compatibility of `settings.json`, `library.json`, the IPC
protocol and window-mode behavior is not guaranteed.

**(g) Reporting infringement or problems.** The project's only channel is GitHub:
[repository issues](https://github.com/ModernByte/orbitfm/issues). Report
vulnerabilities privately as described in [SECURITY.md](SECURITY.md), section 11;
there is no dedicated rights-holder contact.

## Русский

Это не юридическая консультация: если пункты ниже вызывают сомнения в вашей
юрисдикции, обратитесь к своему юристу.

**(а) «Как есть».** OrbitFM распространяется по лицензии MIT, и её раздел
«DISCLAIMER OF WARRANTY» действует полностью: программа поставляется «КАК ЕСТЬ»,
БЕЗ КАКИХ-ЛИБО ГАРАНТИЙ, явных или подразумеваемых, включая гарантии товарной
пригодности, пригодности для конкретной цели и ненарушения прав. Мы не обещаем,
что приложение работает без ошибок, что выбранная станция заиграет и что
поведение версии `0.x` совпадёт с поведением следующей.

**(б) Ответственность.** В пределах, допускаемых применимым правом, авторы,
контрибьюторы и ModernByte не отвечают за прямые, косвенные, случайные, особые и
штрафные убытки, упущенную выгоду, утрату данных, прерванную работу или ущерб
оборудованию, связанные с использованием программы или невозможностью её
использовать. В некоторых юрисдикциях такие ограничения недопустимы, поэтому
часть из них может к вам не применяться (в США дисклеймер должен быть заметным —
см. [рекомендации FTC](https://www.ftc.gov/business-guidance/resources/using-conspicuous-language-make-effective-online-redisclaimers)).

**(в) Станции и потоки.** Приложение ничего не транслирует, не записывает и не
хранит аудио. Списки станций приходят из стороннего каталога Radio Browser,
который поддерживает его сообщество; мы его не модерируем и не отвечаем за
содержимое. Права на контент конкретной станции и легальность её прослушивания в
вашей стране вы проверяете сами: ответственность за нарушение авторских прав и за
соблюдение условий использования сервисов лежит на пользователе.

**(г) Сеть и данные.** Программа обращается только к серверам Radio Browser
(зеркала, разрешённые через `all.api.radio-browser.info`, включая запись о
прослушивании `/json/url/<uuid>`) и к адресам, которые выбрали вы: потоку станции
и её фавикону. Ваш IP-адрес виден вещателю — как при любом сетевом запросе.
Аккаунтов, облачной синхронизации и телеметрии нет: в коде нет адресов
аналитики, настройки, избранное и история лежат в ваших каталогах. Профиль сети
меняет объём и частоту фоновых запросов, но не их адресатов и не политику
проверки URL. Появление телеметрии потребует отдельного архитектурного решения и
явного включения пользователем.

**(д) Товарные знаки.** Noctalia, niri, Hyprland, mpv, Radio Browser, Rust,
Windows, macOS и другие названия и логотипы принадлежат их владельцам; упоминание
не означает аффилиации, одобрения или спонсорства.

**(е) Статус пре-релиза.** До 1.0.0 приложение выходит как `0.x`, и совместимость
форматов `settings.json` и `library.json`, протокола IPC и поведения режимов окна
не гарантируется. Правила версионирования и история изменений публикуются вместе
с релизами.

**(ж) Сообщить о нарушении прав или о проблеме.** Единственный канал проекта —
GitHub: [задачи репозитория](https://github.com/ModernByte/orbitfm/issues).
Об уязвимостях сообщайте приватно, как описано в [SECURITY.md](SECURITY.md),
раздел 11; отдельного контакта для правообладателей нет.

---

Лицензия MIT © 2026 ModernByte — полный текст в файле [LICENSE](LICENSE). Именно
он задаёт отказ от гарантий и ограничение ответственности; раздел выше его
расширяет, но не отменяет.
