// The settings: how a signal looks, when it may come, own devices, the account.
import {
  html, render, useState, useEffect, useRef, invoke, useModel, useDark, useFit, quietBrowser, SITE, tr, LANG,
  Icon, Avatar, Caption, Note, Field, SwitchRow, LinkRow, InviteRow, SwatchPicker, palette,
} from './lib.js';

/// The site's languages by their own names; "" follows Windows.
const LANGUAGES = [['en', 'English'], ['es', 'Español'], ['pt', 'Português'], ['de', 'Deutsch'], ['fr', 'Français'],
  ['it', 'Italiano'], ['pl', 'Polski'], ['tr', 'Türkçe'], ['uk', 'Українська'], ['ru', 'Русский'],
  ['id', 'Bahasa Indonesia'], ['ko', '한국어'], ['zh', '中文']];

const TABS = [['signal', tr('Сигнал')], ['when', tr('Когда')], ['devices', tr('Устройства')], ['account', tr('Аккаунт')]];

function Settings() {
  const model = useModel();
  useDark();
  // In the address, so the page comes back on the same tab after a change of language.
  const [tab, setTabState] = useState(() => location.hash.slice(1) || 'signal');
  const setTab = (id) => {
    history.replaceState(null, '', '#' + id);
    setTabState(id);
  };
  const root = useRef();
  useFit(root, () => invoke('ready'));

  useEffect(() => {
    const key = (e) => e.key === 'Escape' && invoke('hide');
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  }, []);

  let page = null;
  if (model) {
    if (tab === 'signal') page = html`<${Signal} model=${model} />`;
    if (tab === 'when') page = html`<${When} model=${model} />`;
    if (tab === 'devices') page = html`<${Devices} model=${model} />`;
    if (tab === 'account') page = html`<${Account} model=${model} />`;
  }
  return html`<div ref=${root} class="col" style="gap: 16px; padding: 16px 20px 20px">
    <div class="tabs">
      ${TABS.map(([id, title]) => html`<button class=${tab === id ? 'on' : ''} onClick=${() => setTab(id)}>${title}</button>`)}
    </div>
    ${page}
  </div>`;
}

const set = (key, value) => invoke('set_setting', { key, value });

// ---- Сигнал ----------------------------------------------------------------------

function Signal({ model }) {
  const s = model.settings;
  const p = palette(s.myColor);
  return html`<div class="col" style="gap: 18px">
    <div class="col" style="gap: 8px">
      <div class="row"><${Caption}>${tr('Мой цвет сигнала')}<//><span class="grow"></span><span class="small" style=${{ color: p.deep }}>${p.name}</span></div>
      <${SwatchPicker} value=${s.myColor} onChange=${(i) => set('myColor', i)} size=${32} />
      <${Note}>${tr('Этим цветом светятся края экрана, когда кто-то о тебе думает.')}<//>
    </div>

    <div class="col" style="gap: 4px">
      <div class="row">
        <${Caption}>${tr('Смайликов за раз')}<//><span class="grow"></span>
        <span class="small muted">${s.emojiCount === 0 ? tr('только свет') : s.emojiCount}</span>
      </div>
      <input type="range" min="0" max="60" step="2" value=${s.emojiCount} disabled=${s.calm}
        onInput=${(e) => set('emojiCount', Number(e.target.value))} />
      ${s.calm && html`<${Note}>${tr('С «Без мигания и полёта» смайлики не летают.')}<//>`}
    </div>

    <div class="card">
      <${SwitchRow} title=${tr('Тихий звук')} on=${s.sound} onChange=${(v) => set('sound', v)} />
      <hr style="margin: 0" />
      <${SwitchRow} title=${tr('Без мигания и полёта')} on=${s.calm} onChange=${(v) => set('calm', v)} />
    </div>

    <button class="play" onClick=${() => invoke('play_effect')}><${Icon} name="play" />${tr('Показать, как это выглядит')}</button>
  </div>`;
}

// ---- Когда -----------------------------------------------------------------------

const GAPS = [1, 5, 15, 30, 60, 300, 900, 3600];

const gapTitle = (gap) => (gap < 60 ? tr('{n} с', { n: gap }) : gap < 3600 ? tr('{n} мин', { n: gap / 60 }) : tr('{n} ч', { n: gap / 3600 }));

function nearestGap(gap) {
  let best = 0;
  GAPS.forEach((g, i) => {
    if (Math.abs(g - gap) < Math.abs(GAPS[best] - gap)) best = i;
  });
  return best;
}

function When({ model }) {
  const s = model.settings;
  const gap = nearestGap(s.minGap);
  return html`<div class="col" style="gap: 18px">
    <div class="col" style="gap: 8px">
      <${Caption}>${model.paused ? model.pauseText : tr('Пауза')}<//>
      ${model.paused
        ? html`<button class="secondary" onClick=${() => invoke('resume')}>${tr('Снять паузу')}</button>`
        : html`<div class="row" style="gap: 8px">
            <button class="secondary grow" onClick=${() => invoke('pause', { length: 'hour' })}>${tr('На час')}</button>
            <button class="secondary grow" onClick=${() => invoke('pause', { length: 'untilMorning' })}>${tr('До утра')}</button>
            <button class="secondary grow" onClick=${() => invoke('pause', { length: 'untilResumed' })}>${tr('Пока не сниму')}</button>
          </div>`}
    </div>

    <div class="col" style="gap: 4px">
      <div class="row"><${Caption}>${tr('Свечение не чаще раза в')}<//><span class="grow"></span><span class="small muted">${gapTitle(GAPS[gap])}</span></div>
      <input type="range" min="0" max=${GAPS.length - 1} step="1" value=${gap}
        onInput=${(e) => set('minGap', GAPS[Number(e.target.value)])} />
      <${Note}>${tr('Если о тебе думают часто, сигналы между вспышками не покажутся.')}<//>
    </div>

    <div class="card">
      <${SwitchRow} title=${tr('Тихо, когда что-то на весь экран')} on=${s.quietWhenPresenting} onChange=${(v) => set('quietWhenPresenting', v)} />
      <hr style="margin: 0" />
      <${SwitchRow} title=${tr('Запускать при входе в Windows')} on=${model.launchAtLogin} onChange=${(v) => set('launchAtLogin', v)} />
    </div>

    <div class="col" style="gap: 6px; padding: 4px 0 2px; border-radius: 10px; background: var(--anyone-3)">
      <${SwitchRow} title=${tr('Сердечки от кого-нибудь на свете')} on=${s.acceptAnyone} onChange=${(v) => set('acceptAnyone', v)} />
      <${Note} style="padding: 0 12px 10px">${tr('Кто-то где-то нажал «Кому-нибудь на свете», и сердечко может прилететь тебе. Бывает редко, тем и приятнее.')}<//>
    </div>

    ${model.problem && html`<div class="small danger">${model.problem}</div>`}
  </div>`;
}

// ---- Устройства --------------------------------------------------------------------

const relative = new Intl.RelativeTimeFormat(LANG, { numeric: 'auto' });

/// "5 минут назад", "вчера".
function ago(iso) {
  const seconds = (new Date(iso).getTime() - Date.now()) / 1000;
  for (const [unit, size] of [['day', 86400], ['hour', 3600], ['minute', 60]]) {
    if (Math.abs(seconds) >= size) return relative.format(Math.round(seconds / size), unit);
  }
  return tr('только что');
}

/// "Принимает и отправляет · был онлайн 5 минут назад".
function details(device) {
  const receives = device.capabilities.includes('receive'), sends = device.capabilities.includes('send');
  const text = receives && sends ? tr('Принимает и отправляет') : receives ? tr('Принимает') : sends ? tr('Отправляет') : '';
  return text + (device.last_seen_at ? tr(' · был онлайн {ago}', { ago: ago(device.last_seen_at) }) : tr(' · ещё не подключался'));
}

function Devices({ model }) {
  const [devices, setDevices] = useState(null);
  const [failed, setFailed] = useState(false);
  const [name, setName] = useState('');
  const [receives, setReceives] = useState(true);
  const [sends, setSends] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(null);
  const [removing, setRemoving] = useState(null);
  const [copied, setCopied] = useState(false);

  const load = () => invoke('devices').then((list) => { setDevices(list); setFailed(false); }, () => setFailed(true));
  useEffect(() => { load(); }, []);

  const add = async () => {
    const capabilities = [...(receives ? ['receive'] : []), ...(sends ? ['send'] : [])];
    if (!name.trim() || capabilities.length === 0 || busy) return;
    setBusy(true);
    setError(null);
    try {
      await invoke('add_device', { name: name.trim(), capabilities });
      setName('');
      load();
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };

  const remove = async (id) => {
    try {
      await invoke('remove_device', { id });
      setRemoving(null);
      load();
    } catch {
      setRemoving(null);
      setError(tr('Не получилось отключить, попробуй ещё раз'));
    }
  };

  const created = model.createdDevice;
  if (created) {
    const copy = () => {
      invoke('copy', { text: created.token });
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    };
    return html`<div class="col" style="gap: 12px">
      <div class="title">${tr('«{name}» подключено', { name: created.name })}</div>
      <div class="card selectable" style="padding: 10px 12px; font-family: Consolas, monospace; font-size: 12.5px; word-break: break-all; -webkit-user-select: text; user-select: text">${created.token}</div>
      <${Note}>${tr('Это токен устройства. Мы покажем его только сейчас: скопируй и вставь в прошивку. С ним можно отправлять сердечки от твоего имени, так что никому не показывай.')}<//>
      <div class="row" style="gap: 8px">
        <button class="primary grow" onClick=${copy}><${Icon} name=${copied ? 'check' : 'copy'} />${copied ? tr('Скопировано') : tr('Скопировать токен')}</button>
        <button class="secondary grow" onClick=${() => invoke('forget_created_device')}>${tr('Готово')}</button>
      </div>
    </div>`;
  }

  const canAdd = name.trim() && (receives || sends) && !busy;
  return html`<div class="col" style="gap: 16px">
    <${Note}>${tr('Игрушка, лампа или своя плата, которая светится, когда о тебе думают, или отправляет сердечко по кнопке.')}<//>

    <div class="col" style="gap: 10px">
      <${Field} title=${tr('Название')}>
        <input value=${name} maxlength="40" placeholder=${tr('Например, Котик на полке')} onInput=${(e) => setName(e.target.value)}
          onKeyDown=${(e) => e.key === 'Enter' && add()} />
      <//>
      <div class="card">
        <${SwitchRow} title=${tr('Принимает сердечки')} on=${receives} onChange=${setReceives} />
        <hr style="margin: 0" />
        <${SwitchRow} title=${tr('Отправляет сердечки')} on=${sends} onChange=${setSends} />
      </div>
      <button class="primary" disabled=${!canAdd} onClick=${add}>${busy ? tr('Подключаем…') : tr('Подключить устройство')}</button>
      ${error && html`<div class="small danger">${error}</div>`}
    </div>

    ${failed && html`<div class="small danger">${tr('Не получилось загрузить устройства')}</div>`}
    ${devices && devices.length > 0 && html`<div class="col" style="gap: 6px">
      <${Caption}>${tr('Подключены')}<//>
      <div class="card">
        ${devices.map((d, i) => html`<div key=${d.id}>
          ${i > 0 && html`<hr style="margin: 0" />`}
          <div class="row" style="gap: 10px; padding: 8px 12px; min-height: 44px">
            <div class="col grow">
              <span class="ellipsis" style="font-size: 13.5px">${d.name}</span>
              <span class="note">${details(d)}</span>
            </div>
            ${removing === d.id
              ? html`<button class="pill" onClick=${() => setRemoving(null)}>${tr('Нет')}</button>
                <button class="pill danger-fill" onClick=${() => remove(d.id)}>${tr('Отключить')}</button>`
              : html`<button class="icon-button faint" title=${tr('Отключить')} aria-label=${tr('Отключить')} onClick=${() => setRemoving(d.id)}><${Icon} name="trash" /></button>`}
          </div>
        </div>`)}
      </div>
    </div>`}

    <${LinkRow} icon="book" title=${tr('Как подключить свою плату')} onClick=${() => invoke('open_url', { url: SITE + '/docs' })} />
  </div>`;
}

// ---- Аккаунт ---------------------------------------------------------------------

function Account({ model }) {
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(null);

  const remove = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke('delete_account');
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return html`<div class="col" style="gap: 18px">
    <div class="row card" style="gap: 12px; padding: 12px">
      <${Avatar} label=${model.email || '♥'} color=${palette(model.settings.myColor)} size=${38} />
      <div class="col grow">
        <span class="ellipsis" style="font-size: 13.5px; font-weight: 600">${model.email}</span>
        <span class="note">${tr('Книга хранится в аккаунте')}</span>
      </div>
      <button class="small-button" onClick=${() => invoke('sign_out')}>${tr('Выйти')}</button>
    </div>

    <div class="col" style="gap: 6px">
      <${InviteRow} model=${model} />
      <${Note}>${tr('Отправь его тем, кто хочет нажимать, когда думает о тебе.')}<//>
    </div>

    <div class="col" style="gap: 8px">
      ${confirm
        ? html`<div class="small">${tr('Удалить аккаунт и весь список? Это навсегда.')}</div>
          <div class="row" style="gap: 8px">
            <button class="pill" onClick=${() => setConfirm(false)} disabled=${busy}>${tr('Не удалять')}</button>
            <button class="pill danger-fill" onClick=${remove} disabled=${busy}>${tr('Удалить навсегда')}</button>
          </div>`
        : html`<div class="row" style="gap: 10px">
            <div class="col grow">
              <span style="font-size: 13.5px">${tr('Удалить аккаунт')}</span>
              <span class="note">${tr('Вместе с книгой, устройствами и Love ID.')}</span>
            </div>
            <button class="small-button danger" onClick=${() => setConfirm(true)}>${tr('Удалить…')}</button>
          </div>`}
      ${error && html`<div class="small danger">${error}</div>`}
    </div>

    <div class="row" style="gap: 10px">
      <span class="grow" style="font-size: 13.5px">${tr('Язык')}</span>
      <select class="language" value=${model.settings.language || ''} onChange=${(e) => set('language', e.target.value)}>
        <option value="">${tr('Как в системе')}</option>
        ${LANGUAGES.map(([code, name]) => html`<option value=${code}>${name}</option>`)}
      </select>
    </div>

    <div class="note center">Love Around ${model.version}</div>
  </div>`;
}

quietBrowser();
render(html`<${Settings} />`, document.getElementById('app'));
