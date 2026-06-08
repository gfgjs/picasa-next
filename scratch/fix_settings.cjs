const fs = require('fs');
const file = 'd:/photoapp/picasa-next/src/views/SettingsView.vue';
let content = fs.readFileSync(file, 'utf8');

// Task 1: missing </div> for hoverScale
content = content.replace(
  '          </label>\r\n        <div class="settings-card__item">',
  '          </label>\r\n        </div>\r\n        <div class="settings-card__item">'
);
content = content.replace(
  '          </label>\n        <div class="settings-card__item">',
  '          </label>\n        </div>\n        <div class="settings-card__item">'
);

// Task 2: gap: 32px -> gap: 48px
content = content.replace(
  /\.settings-content\s*\{[\s\S]*?gap:\s*32px;/,
  match => match.replace('gap: 32px;', 'gap: 48px;')
);

// Task 3: move pin-btn out of settings-card__info
const regex = /(<div class="settings-card__info">\s*<div class="settings-card__label"[^>]*>\s*)(<button class="pin-btn"[\s\S]*?<\/button>\s*)/g;
let count = 0;
content = content.replace(regex, (match, prefix, btn) => {
  count++;
  const infoMatch = prefix.match(/(<div class="settings-card__info">\s*)(<div class="settings-card__label"[^>]*>\s*)/);
  if (infoMatch) {
    let newBtn = btn.trim().split('\n').map((line, idx) => {
      if (idx === 0) return '          ' + line.trim();
      return '            ' + line.trim();
    }).join('\n');
    return newBtn + '\n          ' + infoMatch[1].trim() + '\n            ' + infoMatch[2].trim() + ' ';
  }
  return btn + prefix;
});

fs.writeFileSync(file, content, 'utf8');
console.log('done, replacements:', count);
