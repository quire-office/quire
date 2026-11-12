# vendor 來源登記
- 檔案：renderer.js
- 來源：quire-www repo `public/playground/renderer.js`
- 來源 commit：（PM 放置時）見 quire-www git log
- 理由：編輯器外殼與官網 playground 共用同一顯示列表繪製器，避免雙實作漂移；
  搬進 quire-editor repo 時本檔轉為 @quire/core 內建繪製器，官網改引用 npm 包。
