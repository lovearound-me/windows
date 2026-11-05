// What every page shares: the bridge to the app, the palette, and the small
// pieces the screens are built from. The app (Rust) holds the state and sends
// it whole on every change; the pages only draw it and ask for changes.
import { html, render, useState, useEffect, useRef, useLayoutEffect } from './vendor/preact-htm.js';

export { html, render, useState, useEffect, useRef, useLayoutEffect };

export const SITE = 'https://lovearound.me';

const tauri = window.__TAURI__;

export function invoke(command, args) {
  return tauri.core.invoke(command, args);
}

// ---- language -------------------------------------------------------------------

/// The app's language and its dictionary (i18n/<lang>.json, keyed by the
/// Russian text), with English under it for what is missing. A page is
/// loaded again when the language changes (useModel).
const STRINGS = await invoke('strings');
export const LANG = STRINGS.lang;
document.documentElement.lang = LANG;
const plurals = new Intl.PluralRules(LANG);

const fill = (text, args) => text.replace(/\{(\w+)\}/g, (all, key) => (args && key in args ? String(args[key]) : all));

const lookup = (text) => {
  for (const table of [STRINGS.table, STRINGS.fallback]) {
    if (table && text in table) return [table[text], table === STRINGS.table ? LANG : 'en'];
  }
  return [text, 'ru'];
};

/// tr('Пауза до {time}', { time: '08:00' })
export function tr(text, args) {
  const [value] = lookup(text);
  return fill(typeof value === 'string' ? value : text, args);
}

/// trn('{n} человек', 5): the form for `n` by the plural rules of the language.
export function trn(text, n, args) {
  const [value, lang] = lookup(text);
  let form = text;
  if (value && typeof value === 'object') {
    const category = (lang === LANG ? plurals : new Intl.PluralRules(lang)).select(n);
    form = value[category] ?? value.other ?? value.many ?? text;
  }
  return fill(form, { ...args, n });
}

/// The app's state, kept current.
export function useModel() {
  const [model, setModel] = useState(null);
  useEffect(() => {
    const update = (m) => (m.lang && m.lang !== LANG ? location.reload() : setModel(m));
    invoke('state').then(update);
    const off = tauri.event.listen('state', (event) => update(event.payload));
    return () => off.then((f) => f());
  }, []);
  return model;
}

/// Redraws every `ms`, for what changes with time (the hour of "Кому-нибудь").
export function useNow(ms) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(timer);
  }, [ms]);
  return now;
}

const darkQuery = matchMedia('(prefers-color-scheme: dark)');

/// Follows the system's light or dark mode.
export function useDark() {
  const [dark, setDark] = useState(darkQuery.matches);
  useEffect(() => {
    const changed = () => setDark(darkQuery.matches);
    darkQuery.addEventListener('change', changed);
    return () => darkQuery.removeEventListener('change', changed);
  }, []);
  return dark;
}

/// The window takes the height of the page: the popover and the settings.
/// `onFirst` runs once the first size is set.
export function useFit(ref, onFirst) {
  useLayoutEffect(() => {
    let first = true;
    const fit = () => {
      const height = Math.ceil(ref.current.getBoundingClientRect().height);
      invoke('fit', { height }).then(() => {
        if (first) {
          first = false;
          onFirst?.();
        }
      });
    };
    const observer = new ResizeObserver(fit);
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, []);
}

// ---- palette -------------------------------------------------------------------

/// The twelve warm colors: `color` for rings and the glow, `tint` for row
/// backgrounds, `deep` for text on the tint. Indices are what settings store.
const PALETTE = [
  [tr('Коралловый'), 0xF07A62, 0xFDE6E0, 0xA8402B],
  [tr('Розовый'), 0xE86A9A, 0xFBE2EC, 0xA3305E],
  [tr('Голубой'), 0x4E9BD8, 0xE0EEF9, 0x22679E],
  [tr('Шалфей'), 0x6FAE7C, 0xE3F1E6, 0x39744A],
  [tr('Лавандовый'), 0x9B83DD, 0xECE6FA, 0x5E45A3],
  [tr('Медовый'), 0xE5A93B, 0xFBF0DA, 0x8A5E0E],
  [tr('Бирюзовый'), 0x3FB3A8, 0xDDF3F0, 0x1D6F68],
  [tr('Вишнёвый'), 0xC94A5E, 0xF8E1E5, 0x8E2638],
  [tr('Карамельный'), 0xC98B5E, 0xF6E9DE, 0x7D4F2C],
  [tr('Оливковый'), 0x9DA24A, 0xEEF0DC, 0x5C601F],
  [tr('Индиго'), 0x6474D6, 0xE4E7F8, 0x3640A0],
  [tr('Грифельный'), 0x7E8C99, 0xE8ECEF, 0x46525E],
];
export const PALETTE_SIZE = PALETTE.length;

const hex = (c) => '#' + c.toString(16).padStart(6, '0');

/// `a` with `t` of it turned into `b`, channel by channel.
function mix(a, b, t) {
  let result = 0;
  for (const shift of [16, 8, 0]) {
    const x = (a >> shift) & 0xff;
    const y = (b >> shift) & 0xff;
    result |= Math.round(x + (y - x) * t) << shift;
  }
  return result;
}

/// The pastel tint on light paper, a dim wash of the color on dark.
export function palette(index) {
  const [name, color, tint, deep] = PALETTE[((index % PALETTE.length) + PALETTE.length) % PALETTE.length];
  const dark = darkQuery.matches;
  return {
    name: tr(name),
    ring: hex(color),
    tint: hex(dark ? mix(color, 0x2a211d, 0.3) : tint),
    deep: hex(dark ? mix(color, 0xffffff, 0.45) : deep),
  };
}

// ---- the model's derived lists ---------------------------------------------------

const byName = (a, b) => a.label.localeCompare(b.label, LANG, { numeric: true, sensitivity: 'base' });

/// Circles on top of the menu, in the order set by hand.
export const closeContacts = (m) => m.contacts.filter((c) => c.close);

/// Everyone else, the most recently signalled first, then by name.
export function otherContacts(m) {
  const last = m.settings.lastSent;
  return m.contacts
    .filter((c) => !c.close)
    .sort((a, b) => (last[b.id] ?? 0) - (last[a.id] ?? 0) || byName(a, b));
}

/// The whole book by name.
export const contactsByName = (m) => [...m.contacts].sort(byName);

export function colorIndex(m, contact) {
  return m.settings.contactColors[contact.id] ?? Math.max(0, m.contacts.findIndex((c) => c.id === contact.id));
}

export const colorFor = (m, contact) => palette(colorIndex(m, contact));

/// The first color not yet used, for a new contact.
export function nextColor(m) {
  const used = new Set(m.contacts.map((c) => m.settings.contactColors[c.id] ?? -1));
  for (let i = 0; i < PALETTE.length; i++) if (!used.has(i)) return i;
  return m.contacts.length % PALETTE.length;
}

/// New contacts start among the close ones until there are a couple of rows of them.
export const newContactIsClose = (m) => closeContacts(m).length < 10;

/// Whole minutes until the next signal to anyone, or null if it can go now.
export function anyoneWait(m, now = Date.now()) {
  if (m.settings.anyoneSentAt == null) return null;
  const left = m.settings.anyoneSentAt + 3600_000 - now;
  return left > 0 ? Math.ceil(left / 60_000) : null;
}

/// "23 человека", "2 человека", "5 человек".
export const peopleCount = (n) => trn('{n} человек', n);

export const firstLetter = (label) => (Array.from(label.trim())[0] ?? '♥').toUpperCase();

export const contains = (label, query) => label.toLocaleLowerCase(LANG).includes(query.toLocaleLowerCase(LANG));

/// The code from an invite link (or the bare code), as typed: 10 characters
/// of the server's alphabet, dashes anywhere. The server never says whether a
/// code is taken, so this is the only check a typo gets.
export function inviteId(text) {
  let rest = text.trim();
  for (const marker of ['/i/', 'lovearound://add/', 'needed://add/']) {
    const at = rest.lastIndexOf(marker);
    if (at >= 0) rest = rest.slice(at + marker.length);
  }
  rest = rest.split(/[/?#]/)[0];
  const code = rest.toUpperCase().replaceAll('-', '');
  return code.length === 10 && /^[23456789ABCDEFGHJKMNPQRSTVWXYZ]+$/.test(code) ? rest : null;
}

// ---- icons ---------------------------------------------------------------------

const ICONS = {
  book: '<path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20"/>',
  gear: '<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>',
  wifiOff: '<path d="M12 20h.01"/><path d="M8.5 16.43a5 5 0 0 1 7 0"/><path d="M5 12.86a10 10 0 0 1 5.17-2.69"/><path d="M19 12.86a10 10 0 0 0-2-1.52"/><path d="M2 8.82a15 15 0 0 1 4.18-2.64"/><path d="M22 8.82a15 15 0 0 0-11.29-3.76"/><path d="m2 2 20 20"/>',
  search: '<circle cx="11" cy="11" r="7.5"/><path d="m20.5 20.5-4.2-4.2"/>',
  plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
  plusCircle: '<circle cx="12" cy="12" r="10"/><path d="M8 12h8"/><path d="M12 8v8"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  alert: '<circle cx="12" cy="12" r="10"/><path d="M12 7.5v5.5"/><path d="M12 16.5h.01"/>',
  copy: '<rect x="8" y="8" width="14" height="14" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
  chevronLeft: '<path d="m15 18-6-6 6-6"/>',
  trash: '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2"/>',
  globe: '<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>',
  share: '<path d="M12 3v12"/><path d="m8 7 4-4 4 4"/><path d="M5 12v7a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-7"/>',
  sparkles: '<path d="M9.94 15.5A2 2 0 0 0 8.5 14.06l-6.14-1.58a.5.5 0 0 1 0-.96L8.5 9.94A2 2 0 0 0 9.94 8.5l1.58-6.14a.5.5 0 0 1 .96 0l1.58 6.14a2 2 0 0 0 1.44 1.44l6.14 1.58a.5.5 0 0 1 0 .96l-6.14 1.58a2 2 0 0 0-1.44 1.44l-1.58 6.14a.5.5 0 0 1-.96 0z"/><path d="M20 3v4"/><path d="M22 5h-4"/><path d="M4 17v2"/><path d="M5 18H3"/>',
  xCircle: { filled: true, d: '<path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm3.7 12.3-1.4 1.4L12 13.4l-2.3 2.3-1.4-1.4 2.3-2.3-2.3-2.3 1.4-1.4 2.3 2.3 2.3-2.3 1.4 1.4-2.3 2.3Z"/>' },
  heart: { filled: true, d: '<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>' },
  moon: { filled: true, d: '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>' },
  play: { filled: true, d: '<path d="M7 4v16l13-8Z"/>' },
};

export function Icon({ name, size, style }) {
  const icon = ICONS[name];
  const filled = typeof icon === 'object';
  return html`<svg class=${'icon' + (filled ? ' filled' : '')} viewBox="0 0 24 24" aria-hidden="true"
    style=${{ ...(size ? { width: size + 'px', height: size + 'px' } : {}), ...style }}
    dangerouslySetInnerHTML=${{ __html: filled ? icon.d : icon }} />`;
}

// ---- pieces --------------------------------------------------------------------

export function Avatar({ label, color, size }) {
  return html`<div class="avatar" style=${{
    width: size + 'px', height: size + 'px', fontSize: size * 0.41 + 'px',
    color: color.deep, boxShadow: `inset 0 0 0 2.5px ${color.ring}`,
  }}>${firstLetter(label)}</div>`;
}

export const Caption = ({ children }) => html`<div class="caption">${children}</div>`;

/// The small grey explanation under a setting.
export const Note = ({ children, style }) => html`<div class="note" style=${style}>${children}</div>`;

export function Field({ title, children }) {
  return html`<label class="field"><span class="caption">${title}</span>${children}</label>`;
}

export function SearchField({ value, onInput, placeholder }) {
  return html`<div class="search">
    <${Icon} name="search" />
    <input value=${value} placeholder=${placeholder} onInput=${(e) => onInput(e.target.value)} spellcheck="false" />
    ${value && html`<button class="faint row" aria-label=${tr('Очистить')} onClick=${() => onInput('')}><${Icon} name="xCircle" /></button>`}
  </div>`;
}

/// Title on the left, switch on the right edge, whatever the title's length.
export function SwitchRow({ title, on, onChange, style }) {
  return html`<div class="switch-row" style=${style}>
    <span class="grow">${title}</span>
    <button class=${'switch' + (on ? ' on' : '')} role="switch" aria-checked=${on} aria-label=${title} onClick=${() => onChange(!on)}></button>
  </div>`;
}

export function IconButton({ icon, help, onClick }) {
  return html`<button class="icon-button" title=${help} aria-label=${help} onClick=${onClick}><${Icon} name=${icon} /></button>`;
}

export function LinkRow({ icon, title, onClick }) {
  return html`<button class="link-row" onClick=${onClick}><${Icon} name=${icon} /><span>${title}</span></button>`;
}

export function Banner({ icon, text, children }) {
  return html`<div class="banner"><${Icon} name=${icon} /><span class="grow">${text}</span>${children}</div>`;
}

export function SwatchPicker({ value, onChange, size }) {
  return html`<div class="swatches" style=${{ '--cell': size + 6 + 'px' }}>
    ${PALETTE.map((_, i) => {
      const p = palette(i);
      return html`<button class=${'swatch' + (value === i ? ' on' : '')} style=${{ '--ring': p.ring }}
        title=${p.name} aria-label=${p.name} aria-pressed=${value === i} onClick=${() => onChange(i)}>
        <span style=${{ background: p.ring }}></span>
      </button>`;
    })}
  </div>`;
}

export const INVITE_MESSAGE = tr('Добавь меня по этой ссылке и нажимай сердечко, когда думаешь обо мне ♥');

/// The link without the scheme: "lovearound.me/i/…" reads better than a URL.
const shownLink = (link) => link.replace('https://', '').replace('http://', '');

/// The own link as one quiet card: the link itself, copied on a click.
export function InviteRow({ model }) {
  const [copied, setCopied] = useState(false);
  if (!model.inviteUrl) return null;
  const copy = () => {
    invoke('copy', { text: model.inviteUrl });
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return html`<button class="invite" onClick=${copy} aria-label=${tr('Скопировать свой Love ID')}
      title=${tr('Кто добавит тебя по этой ссылке, сможет нажимать, когда думает о тебе')}>
    <span class="badge"><${Icon} name="heart" /></span>
    <span class="col grow">
      <span class="muted" style="font-size: 11px">${tr('Твой Love ID')}</span>
      <span class="code ellipsis">${shownLink(model.inviteUrl)}</span>
    </span>
    <span class=${'action' + (copied ? ' copied' : '')}>
      <${Icon} name=${copied ? 'check' : 'copy'} /><span class="label">${copied ? tr('Скопировано') : tr('Скопировать')}</span>
    </span>
  </button>`;
}

/// Adding someone by their invite link: the popover's "Новый человек" and the
/// book's "Добавить по ссылке". The server never says whether a code belongs
/// to anyone (so codes can't be probed): only its shape is checked here.
export function ContactForm({ model, invite, onDone, onAdded }) {
  const [link, setLink] = useState('');
  const [name, setName] = useState('');
  const [color, setColor] = useState(() => nextColor(model));
  const [close] = useState(() => newContactIsClose(model));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    // Copied from a message a moment ago: no need to paste it.
    if (invite) setLink(`${SITE}/i/${invite}`);
    else invoke('pasted_invite').then((code) => code && setLink((now) => now || `${SITE}/i/${code}`));
  }, [invite]);

  const label = name.trim();
  const code = link.trim() ? inviteId(link) : null;
  const canSave = label.length >= 1 && label.length <= 64 && code;

  const save = async () => {
    if (!canSave || busy) return;
    setBusy(true);
    setError(null);
    try {
      const id = await invoke('add_contact', { code, label, color, close });
      onAdded?.(id);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const p = palette(color);
  return html`<div class="col" style="gap: 14px">
    ${onDone && html`<div class="row" style="gap: 6px">
      <${IconButton} icon="chevronLeft" help=${tr('Назад')} onClick=${onDone} />
      <span class="title">${tr('Новый человек')}</span>
    </div>`}

    <${Field} title=${tr('Ссылка-приглашение')}>
      <input value=${link} placeholder="lovearound.me/i/…" spellcheck="false" onInput=${(e) => setLink(e.target.value)} />
    <//>
    <div style="font-size: 12.5px; margin-top: -6px">
      ${!link.trim()
        ? html`<span class="faint">${tr('Вставь ссылку, которую тебе прислали')}</span>`
        : code
          ? html`<span class="ok row" style="gap: 5px"><${Icon} name="check" />${tr('Ссылка в порядке. Осталось подписать.')}</span>`
          : html`<span class="muted">${tr('Не похоже на ссылку-приглашение')}</span>`}
    </div>

    <${Field} title=${tr('Как назовёшь?')}>
      <input value=${name} placeholder=${tr('Например, Мама')} onInput=${(e) => setName(e.target.value)}
        onKeyDown=${(e) => e.key === 'Enter' && save()} />
    <//>

    <div class="col" style="gap: 8px">
      <div class="row"><${Caption}>${tr('Цвет')}<//><span class="grow"></span><span class="small" style=${{ color: p.deep }}>${p.name}</span></div>
      <${SwatchPicker} value=${color} onChange=${setColor} size=${30} />
    </div>

    <button class="primary" disabled=${!canSave || busy} onClick=${save}>${busy ? tr('Сохраняем…') : tr('Добавить')}</button>
    ${error && html`<div class="small danger">${error}</div>`}
    <div class="note" style="margin-top: -4px">
      ${tr('{name} не увидит ни имя, ни цвет. Кружки можно переставлять перетаскиванием.', { name: label || tr('Человек') })}
    </div>
  </div>`;
}

/// A pointer drag that tells a drag from a click: past 4 px it is a drag,
/// and the click that ends it is swallowed.
export function startDrag(event, { onStart, onMove, onEnd }) {
  if (event.button !== 0) return;
  const target = event.currentTarget;
  const [x0, y0] = [event.clientX, event.clientY];
  let dragging = false;
  target.setPointerCapture(event.pointerId);
  const move = (e) => {
    if (!dragging && Math.hypot(e.clientX - x0, e.clientY - y0) > 4) {
      dragging = true;
      onStart?.(e);
    }
    if (dragging) onMove?.(e);
  };
  const up = (e) => {
    target.removeEventListener('pointermove', move);
    target.removeEventListener('pointerup', up);
    target.removeEventListener('pointercancel', up);
    if (dragging) {
      onEnd?.(e);
      const swallow = (c) => { c.stopPropagation(); c.preventDefault(); };
      window.addEventListener('click', swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener('click', swallow, { capture: true }), 0);
    }
  };
  target.addEventListener('pointermove', move);
  target.addEventListener('pointerup', up);
  target.addEventListener('pointercancel', up);
}

/// Ctrl+R and the browser's own menu mean nothing in an app window.
export function quietBrowser() {
  addEventListener('contextmenu', (e) => {
    if (!e.target.closest('input, textarea, .selectable')) e.preventDefault();
  });
  addEventListener('keydown', (e) => {
    if (e.key === 'F5' || (e.ctrlKey && ['r', 'p', 'f', 'g', 'j', 'u'].includes(e.key.toLowerCase())) || (e.ctrlKey && e.shiftKey && e.key === 'I')) {
      e.preventDefault();
    }
  });
}
