# Quire 視覺識別

標誌概念：**書帖（quire）摺出的 Q**——一張紙右上角往內摺（狗折）、右下角抽出一頁成尾，
即為字母 Q，也是產品本身：把 DOCX 重新摺成乾淨版面的引擎。

## 檔案

| 檔 | 用途 |
|---|---|
| `quire-mark.svg` | 標誌母檔（彩色：墨 `#111111` ＋印泥紅 `#C43120`） |
| `quire-mark-mono.svg` | 單色版（文件、單色印刷、鏤空） |
| `quire-mark-white.svg` | 反白版（深色底） |
| `quire-mark-reverse.svg` | 紙色反白＋紅折面（炭灰底首選） |
| `lockup-light.png` / `lockup-dark.png` | 橫式 lockup（標誌＋字標＋副標），明／暗 |
| `lockup-mono.png` | lockup 單色無副標版 |
| `banner.png` | README 頁首 banner（1600×400） |
| `social-preview.png` | GitHub social preview（1280×640，深色） |
| `avatar.png` | 組織 avatar（圓形構圖，400px） |
| `favicon-*.png` / `favicon.ico` | 站點圖標（16/32 為二值硬邊版） |
| `mark-color.png` | 標誌彩色 PNG（512px） |

## 色彩

| 角色 | 色值 |
|---|---|
| 墨（主色） | `#111111` |
| 印泥紅（輔助，僅折面） | `#C43120` |
| 紙（暖白） | `#FAF8F4` |
| 炭灰（深色底） | `#181A1E` |

## 規則

- 折面紅是**唯一**輔助色應用，不上任何其他元素。
- ≤32px 一律單色版（紅折面在小尺寸會髒），48px 以上才啟用彩色。
- 字標採 Source Serif 4 SemiBold（OFL 1.1），副標採 IBM Plex Mono Medium（OFL 1.1）。
- 標誌四周留白 ≥ 標誌寬度的 1/4。
- 禁止：漸層、陰影、圓角化、拉長、為折面以外的部分變色。

## 授權

標誌與插圖採 **CC BY 4.0**；原始碼授權見根目錄 `LICENSE-APACHE`。
字型為 SIL OFL 1.1（來源：Google Fonts）。
