![Followers](https://img.shields.io/github/followers/eoliann?style=plastic&color=green)
![Watchers](https://img.shields.io/github/watchers/eoliann/Foto-acte-3x4?style=plastic)
![Stars](https://img.shields.io/github/stars/eoliann/Foto-acte-3x4?style=plastic)

[![Donate](https://img.shields.io/badge/Donate-PayPal-blue?style=plastic)](https://www.paypal.com/donate/?hosted_button_id=PTH2EXUDS423S)
[![Donate](https://img.shields.io/badge/Donate-Revolut-8A2BE2?style=plastic)](https://revolut.me/adriannm9)
[![Donate](https://img.shields.io/badge/Donate-KoFi-green?style=plastic)](https://ko-fi.com/eoliann)

![Release Date](https://img.shields.io/github/release-date/eoliann/Foto-acte-3x4?style=plastic)
![Last Commit](https://img.shields.io/github/last-commit/eoliann/Foto-acte-3x4?style=plastic)
![GitHub Downloads (all assets, all releases)](https://img.shields.io/github/downloads/eoliann/Foto-acte-3x4/total?style=plastic)
[![Downloads latest](https://img.shields.io/github/downloads/eoliann/Foto-acte-3x4/latest/total?style=plastic)](https://github.com/eoliann/Foto-acte-3x4/releases/latest/download/Foto-acte-3x4.exe)

![OS](https://img.shields.io/badge/OS-Windows-blue?style=plastic)
![Lang](https://img.shields.io/badge/Lang-Rust-magenta?style=plastic)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg?style=plastic)](LICENSE.md)

<details open>
<summary><b>Downloads · daily (cumulative)</b></summary>

<img src=".github/downloads/chart.svg" alt="Daily download progress" width="760">

</details>
<details>
<summary><b>Downloads · monthly</b></summary>

<img src=".github/downloads/chart-monthly.svg" alt="Monthly downloads" width="760">

</details>
<details>
<summary><b>Downloads · yearly</b></summary>

<img src=".github/downloads/chart-yearly.svg" alt="Yearly downloads" width="760">

</details>


# Foto acte 3x4

A portable, offline Windows desktop app that arranges a photo into 6 copies of 3x4 cm on a 15x10 cm photo sheet.

## Features

- JPEG export of 1772x1181 pixels at 300 DPI;
- six photos of 354x472 pixels, about 3x4 cm each;
- manual zoom and position adjustment for precise framing;
- brightness and contrast adjustment;
- AI background removal, fully offline;
- white, light gray, light blue or any custom color from the palette as background;
- sheet preview before saving;
- loading through a file picker or drag-and-drop;
- EXIF orientation correction for photos taken with a phone;
- fully local processing, the photo is never uploaded to the internet.

## Gallery

<p align="center">
  <img src="./galerie/dashboard_1.png" alt="Foto acte 3x4" width="45%">
  <img src="./galerie/dashboard_2.png" alt="Foto acte 3x4" width="45%">
</p>
<p align="center">
  <img src="./galerie/dashboard_3.png" alt="Foto acte 3x4" width="45%">
  <img src="./galerie/dashboard_4.png" alt="Foto acte 3x4" width="45%">
</p>
<p align="center">
  <img src="./galerie/rezultat.jpg" alt="Foto acte 3x4" width="45%">
  <img src="./galerie/print_result.png" alt="Foto acte 3x4" width="45%">
</p>

## Download

Download the Windows archive from the [Releases](../../releases/latest) page, extract `Foto-acte-3x4.exe` and start the app. The executable needs no installation and no Rust on the user's computer.

Windows may show a SmartScreen warning for new executables that are not digitally signed. Make sure the file comes from the Releases page of this repository.

## Usage

1. Open `Foto-acte-3x4.exe`.
2. Load the photo or drag the file onto the window.
3. Adjust the zoom and position until the head and shoulders are framed correctly.
4. Adjust brightness and contrast, remove and set the background.
5. Click **Save for print** (*Salvează pentru print*).
6. Print the JPEG at **100% / Actual size**, without the `Fit to page` option.

The app does not detect or modify the face automatically. Manual framing keeps the result predictable and lets you adapt to each photo.

To replace the background, pick one of the predefined colors or the **Custom** (*Personalizat*) option. The first AI processing can take a few seconds, depending on the processor; all later adjustments are shown immediately.

## Supported formats

JPEG, PNG, WebP and BMP are accepted as input. The result is saved as JPEG with 300 DPI metadata.

## Development

[Rust](https://www.rust-lang.org/tools/install) 1.94 or newer is required.

```powershell
cargo run
```

Local checks:

```powershell
cargo fmt -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

## Portable build

From PowerShell run:

```powershell
.\build-portable.ps1
```

The standalone executable will be created at `dist\Foto-acte-3x4.exe`.

## Publishing a release

The GitHub Actions workflow builds and tests the project on every push and pull request. A tag of the form `v1.1.2` automatically creates a GitHub Release containing the Windows archive:

```powershell
git tag v1.1.2
git push origin v1.1.2
```

## Technologies

The interface is built in Rust with `eframe/egui`, image processing uses the `image` crate, and background removal uses the MODNet model through the `RTen` runtime.

The AI model is embedded in the executable and distributed under the Apache-2.0 license. Its provenance and license are documented in [`assets/README.md`](assets/README.md).
