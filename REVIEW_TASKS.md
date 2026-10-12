# Проверка реализации RUST_MIGRATION_PLAN.md: задачи

Ревью от 2026-10-12, ветка `rust`, коммит `5feac2c` плюс незакоммиченная правка `src/ui/dialogs.rs`.
Каждый пункт независим: его можно взять и закрыть отдельно. После правки кода: `cargo clippy --all-targets`, `cargo test`, `cargo build`, `cargo build --release`.
Если отличие от 1.8.4 оставляем намеренно, оно записывается в «Deliberate differences from 1.8.4» нужной фазы плана.

## Что подтверждено

| Проверка | В плане | Факт |
|---|---|---|
| `cargo test` | 94 теста | 94 passed, 0 failed |
| Объём кода | ~10 200 строк, 34 файла | 10 242 строки, 34 файла |
| Release exe | 8.48 MB | 8 480 256 байт |
| `cargo clippy --all-targets` | без предупреждений | 4 предупреждения, см. задачу 1 |

С `nvencFFX.py` сверены и совпадают: 46 ключей настроек и их слияние, сборщик команды ffmpeg, конвертация, пауза, 10s preview, встроенные и пользовательские пресеты, batch, запись экрана, Streams, VMAF, превью кадра, проверка трима.

---

## Код

### [x] 1. Clippy выдаёт 4 предупреждения

- `src/ui/widgets.rs:594` и `:600`: `collapsible_if`, вложенный `if` надо свернуть в let-chain.
- `src/main.rs:37` и `:81`: `chunks_exact_to_as_chunks`, заменить `chunks_exact(4)` на `as_chunks::<4>()`.
- Скорее всего, это новые линты toolchain 1.99, но CLAUDE.md требует чистый clippy.
- Готово, когда `cargo clippy --all-targets` не выдаёт предупреждений.

### [x] 2. Высота окна не зависит от масштаба экрана

- Где: `src/main.rs:58-60`. Сейчас старт 700, минимум 600, максимум 4096 при любом масштабе.
- 1.8.4, `nvencFFX.py:1505-1519`: при масштабе ниже 125% окно 700, минимум 700, максимум 900. От 125% это 650, 650 и 800. От 200% это 600, 600 и 650.
- Последствие: при масштабе 150% на экране 1080p окно уходит под панель задач. Ручное растягивание не ограничено сверху.
- Сделать: выставить стартовый размер, минимум и максимум по масштабу, как в 1.8.4. Масштаб до создания окна можно взять из DPI монитора. Альтернатива: задать их в первом кадре через `ViewportCommand`. Учесть, что `fitted_height` в `src/app.rs` уже знает те же пределы.
- Или оставить как есть и описать в «Deliberate differences» Фазы 2.

### [x] 3. Окно не центрируется при запуске

- 1.8.4, `_center_window`, `nvencFFX.py:6065`: окно по центру экрана по горизонтали, 50 px от верха.
- Rust не задаёт позицию, её выбирает Windows.
- Сделать: позиция через `ViewportBuilder::with_position` по размеру монитора или первым кадром через `ViewportCommand::OuterPosition`. Или описать отличие в плане.
- Готово: `win32::center_window` (SetWindowPos в физических пикселях, как у CTk) из `NvencFfxApp::new`, пока окно скрыто. Проверено: x 870, y 50 на экране 2560.

### [x] 4. «Unexpected Error» показывает текст исключения

- Где: `src/app.rs:1220`.
- 1.8.4, `nvencFFX.py:5953`: только фраза «An unexpected error occurred during conversion».
- Сделать: убрать `:\n{e}` или записать как намеренное отличие Фазы 4.
- Готово: `:\n{e}` убран, текст как в 1.8.4.

### [x] 5. tune = lossless из пресета или настроек не выключает AQ

- 1.8.4, `nvencFFX.py:1686`: `_on_tune_changed` висит на любой записи в `tune`. Пресет и `nff_settings.json` с lossless выключают Spatial и Temporal AQ и пишут «Note: AQ not supported for lossless encoding».
- Rust вызывает `tune_changed` (`src/app.rs:660`) только из меню и встроенных пресетов. `load_custom_preset` (`src/app.rs:738`) и загрузка при старте этого не делают.
- Сделать: вызвать `tune_changed` после `apply_json` в `load_custom_preset` и после загрузки настроек в `NvencFfxApp::new`.
- Готово: `tune_changed` вызывается в `NvencFfxApp::new`, `load_custom_preset` и в `set_codec` при переходе на H.264 (как `_update_codec_settings`). В 1.8.4 для HEVC и AV1 AQ из пресета оставался включённым: `_apply_settings_dict` пишет флаги AQ после tune. Rust выключает AQ для всех кодеков, это записано в «Deliberate differences» Фазы 2. Тест `lossless_turns_aq_off_after_codec_switch_and_preset_load`.

### [x] 6. VMAF подаёт звук без найденного балла

- Где: `src/app/vmaf.rs:117`. Звук идёт при любом `score.is_some()`, даже если строка не разобралась как число.
- 1.8.4, `nvencFFX.py:3478`: звук только после успешного `float(vmaf_score)`.
- Сделать: звук только когда `result_status` вернул балл.
- Готово: `result_status` возвращает `Ok` только для разобранного балла, звук только при `Ok`. Тест `status_texts` проверяет `Ok` / `Err`.

### [x] 7. Неизвестное audio_option превращается в аргументы

- Где: `src/core/command.rs:80`, ветка `other =>`. Значение вида `x_y` из вручную правленого пресета даёт `-c:a x -b:a y`.
- 1.8.4, `_append_audio_options`, `nvencFFX.py:5815`: обрабатывает только 9 известных значений, остальные игнорирует.
- Сделать: перечислить `aac_96k`, `aac_160k`, `aac_256k`, `opus_96k`, `opus_160k`, `opus_256k` явно, остальное пропускать. Добавить тест.
- Готово: шесть значений перечислены явно, неизвестные ничего не добавляют, как в 1.8.4. Тест `audio_options_known_values_only`.

### [x] 8. Переполнение при огромном битрейте

- Где: `src/core/command.rs:149`, `kbps * 12`. Битрейт из 17 и более цифр переполняет i64.
- В debug это паника, в release мусорный maxrate. Python считал в длинных целых.
- Сделать: `checked_mul` с ошибкой «Video bitrate must be a number.» или ограничить диапазон. Добавить тест.
- Готово: maxrate и bufsize считаются через `checked_mul` / `checked_add`, при переполнении ошибка «Video bitrate must be a number.». Порог на деле 18 цифр, а не 17: переполнение начинается выше 768 614 336 404 564 649. Отличие от 1.8.4 записано в Фазе 4 плана. Тест `huge_bitrate_is_rejected_not_overflowed`.

---

## План

### [x] 9. Раздел 2 «Target Rust Architecture» устарел

- `RUST_MIGRATION_PLAN.md:22-72`.
- Таких файлов нет: `ui/main_view.rs`, `ui/encoder_section.rs`, `ui/fps_scale_section.rs`, `ui/audio_section.rs`, `ui/trim_section.rs`, `ui/presets_section.rs`, `config/defaults.rs`.
- Эти файлы есть, но не перечислены: `ui/dialogs.rs`, `core/filters.rs`, `core/timecode.rs`.
- Сделать: привести дерево к фактическому `src/`.
- Готово: дерево переписано по `src/`: убраны несуществующие `ui/*_section.rs`, `ui/main_view.rs`, `config/defaults.rs`; добавлены `ui/dialogs.rs`, `core/filters.rs`, `core/timecode.rs`, `config/testdata/`, `assets/`; у `app.rs` и `win32/mod.rs` описано фактическое содержимое. В строке `settings.rs` сразу 46 ключей (строка 134 остаётся задачей 11).

### [x] 10. Раздел 3 «Complete Dependency List» не совпадает с Cargo.toml

- `RUST_MIGRATION_PLAN.md:94`: зависимости `open = "5.4"` нет, файлы открываются через `ShellExecuteW`.
- Нет `raw-window-handle = "0.6"` и фичи `preserve_order` у `serde_json`.
- `windows-sys` в Cargo.toml стоит под `[target.'cfg(windows)'.dependencies]`.
- Сделать: переписать блок по текущему Cargo.toml.
- Готово: блок переписан по Cargo.toml в том же порядке: убран `open`, добавлены `raw-window-handle = "0.6"` (HWND окон eframe и родитель файловых диалогов из вторичных viewport), `serde_json` с `preserve_order`, `windows-sys` перенесён под `[target.'cfg(windows)'.dependencies]`, добавлена фича `Win32_UI_HiDpi` (её тоже не было). В заметке под блоком сказано, что он зеркалит Cargo.toml.

### [x] 11. «47 полей» вместо 46

- `RUST_MIGRATION_PLAN.md:59` и `:134`.
- Таблица раздела 4, Фаза 3 и тест `json_keys_follow_the_original_order` говорят 46.
- Сделать: исправить на 46.
- Готово: заголовок раздела 4 исправлен на «46 fields» (в таблице 46 полей, 3+4+2+7+5+5+6+1+4+3+2+3+1). Строка про `settings.rs` в разделе 2 уже говорила 46. Других «47» про ключи настроек в плане, CLAUDE.md и коде нет.

### [x] 12. Раздел 1 упоминает крейт tray-icon

- `RUST_MIGRATION_PLAN.md:18`. Фаза 5 от него отказалась, всё на `windows-sys`.
- Сделать: оставить только `windows-sys`.
- Готово: ячейка теперь «Typed Rust bindings (`windows-sys`)». «Type-safe crates» убрано: `windows-sys` даёт сырые `unsafe` FFI-привязки, они типизированы, но не безопасны. Упоминания `tray-icon` в строках 104 и 276 оставлены, там объяснено, почему он не используется.

### [x] 13. Незакоммиченная иконка в заголовке вторичных окон

- `src/ui/dialogs.rs`: `title_bar` рисует иконку 16×16 из `window_icon()` перед заголовком.
- В плане не описана, текущий release exe уже собран с ней.
- Сделать: дописать в Фазу 1 рядом с пунктом про self-drawn title bar, затем закоммитить.
- Готово: код уже закоммичен в `fe167b6` («cargo clippy», вместе с этим файлом). В Фазу 1 под пунктом про self-drawn title bar добавлен пункт про иконку: 16×16 перед заголовком, из 32×32 записи `nff.ico` через `window_icon()`, одна текстура на все viewport (`dialogs::title_bar_icon`).

---

## Открыто по самому плану

### [x] 14. Third-party notices

- Exe содержит Roboto, шрифты egui и крейты под MIT и Apache-2.0, их лицензии в релиз не попадают.
- Сделать: сгенерировать список, например через `cargo about`, и класть его в `dist` из `build-release.ps1`.
- Готово: `build-release.ps1` вызывает `cargo about generate --locked --fail about.hbs` и кладёт `THIRD-PARTY-NOTICES.txt` (~155 КБ) в `dist\nvencFFX-<version>\` и zip; без `cargo-about` скрипт останавливается и пишет команду установки. `about.toml`: только цель Windows, без build / dev зависимостей, MIT в приоритете; `epaint_default_fonts` уточнён по контрольным суммам, т.к. MIT и Bitstream Vera шрифта Hack лежат в одном файле. Roboto 2.137 (Apache 2.0) вписан в `about.hbs`. В `Cargo.toml` добавлены `license = "MIT"` и `publish = false`, сам nvencffx в список не попадает. Описано в Фазе 9 плана; вкладка License в Help по-прежнему показывает только `nff-license.txt` (риск 12).

### [x] 15. Запись 2.0.0 в CHANGELOG.md

- CHANGELOG начинается с 1.8.4.
- Готово: запись `[2.0.0] - Unreleased` в стиле прежних, разделы Rewritten in Rust / Conversion / Interface / Batch Converter / Screen Recording. Собрана из «Deliberate differences from 1.8.4» всех фаз, только то, что видно пользователю (внутренние вещи вроде отдельного окна для хоткеев не вошли). Дату поставить при релизе, напоминание в «Open before publishing» Фазы 9.

### [ ] 16. Ручные проверки из рисков 6, 10, 11

- Риск 6, запись экрана: рестарт после реальной смены режима дисплея, звук во время воспроизведения, сворачивание, балуны и хоткеи в запущенном приложении.
- Риск 10, batch: ресайз за самописные края, file dialog принадлежит окну batch, drag and drop в него.
- Риск 11, Фаза 8: вид Help и Streams против 1.8.4, позиция превью кадра при 125% и 150% и при прокрутке, фолбэк CUDA на CPU для файла, который NVDEC не декодирует.
