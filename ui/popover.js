// The popover over the tray: who you are thinking of, in one click.
import {
  html, render, useState, useEffect, useRef, invoke, useModel, useNow, useDark, useFit, quietBrowser, tr,
  Icon, Avatar, Caption, SearchField, IconButton, LinkRow, Banner, InviteRow, ContactForm, INVITE_MESSAGE,
  closeContacts, otherContacts, contactsByName, colorFor, anyoneWait, peopleCount, firstLetter, contains, startDrag,
} from './lib.js';

function Root() {
  const model = useModel();
  useDark();
  const root = useRef();
  useFit(root);

  useEffect(() => {
    const key = (e) => e.key === 'Escape' && invoke('hide');
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  }, []);

  let screen = null;
  if (model) {
    if (!model.signedIn) screen = html`<${SignIn} model=${model} />`;
    else if (model.route.name === 'add') {
      const back = () => invoke('set_route', { route: { name: 'main' } });
      screen = html`<${ContactForm} key=${model.route.invite ?? ''} model=${model} invite=${model.route.invite} onDone=${back} onAdded=${back} />`;
    } else screen = html`<${Main} model=${model} />`;
  }
  return html`<div ref=${root} style="padding: 14px">${screen}</div>`;
}

// ---- main ------------------------------------------------------------------------

function Main({ model }) {
  const [query, setQuery] = useState('');
  // Each opening starts from an empty search.
  useEffect(() => {
    const blur = () => setQuery('');
    addEventListener('blur', blur);
    return () => removeEventListener('blur', blur);
  }, []);

  const q = query.trim();
  const others = otherContacts(model);
  // A short book fits whole; a long one shows the recent few.
  const shown = others.length <= 5 ? others : others.slice(0, 4);
  const found = q ? contactsByName(model).filter((c) => contains(c.label, q)) : [];

  return html`<div class="col" style="gap: 8px">
    <div class="row" style="gap: 2px">
      <span class="grow" style="font-size: 15px; font-weight: 600">${tr('О ком ты сейчас думаешь?')}</span>
      <!-- Pause and quitting: a right click on the heart in the tray. -->
      <${IconButton} icon="book" help=${tr('Книга')} onClick=${() => invoke('open_book', {})} />
      <${IconButton} icon="gear" help=${tr('Настройки')} onClick=${() => invoke('open_settings')} />
    </div>

    ${model.offline && html`<${Banner} icon="wifiOff" text=${tr('Нет связи с сервером, сигналы сейчас не приходят. Переподключаемся…')} />`}

    ${!model.settings.receiveHere && html`<${Banner} icon="moon" text=${tr('Этот компьютер не получает сердечки.')}>
      <button class="link" style="font-size: 12.5px; font-weight: 600" onClick=${() => invoke('set_setting', { key: 'receiveHere', value: true })}>${tr('Включить')}</button>
    <//>`}

    ${model.settings.receiveHere && model.paused && html`<${Banner} icon="moon" text=${tr('{pause}. Сигналы не придут.', { pause: model.pauseText })}>
      <button class="link" style="font-size: 12.5px; font-weight: 600" onClick=${() => invoke('resume')}>${tr('Снять')}</button>
    <//>`}

    <${CloseCircles} model=${model} />

    ${model.contacts.length === 0
      ? html`
        <div class="col center" style="gap: 6px; padding-bottom: 6px">
          <div style="font-size: 14px; font-weight: 600">${tr('Пока никого нет')}</div>
          <div class="muted">${tr('Добавь людей по их ссылке или отправь им свой Love ID, чтобы они могли поддерживать тебя.')}</div>
        </div>
        ${model.inviteUrl && html`<${ShareButton} model=${model} />`}`
      : html`
        <${SearchField} value=${query} onInput=${setQuery} placeholder=${tr('Найти — {people}', { people: peopleCount(model.contacts.length) })} />
        ${q
          ? html`<div class="col" style="gap: 2px">
              ${found.map((c) => html`<${ContactRow} key=${c.id} model=${model} contact=${c} />`)}
              ${found.length === 0 && html`<div class="muted center" style="padding: 14px 0">${tr('Никого с таким именем')}</div>`}
            </div>`
          : others.length > 0 && html`<div class="col" style="gap: 2px">
              <div style="padding: 0 10px 2px"><${Caption}>${shown.length < others.length ? tr('Недавние') : tr('Остальные')}<//></div>
              ${shown.map((c) => html`<${ContactRow} key=${c.id} model=${model} contact=${c} />`)}
              ${shown.length < others.length && html`<div style="padding: 0 6px">
                <${LinkRow} icon="book" title=${tr('Вся книга · {n}', { n: model.contacts.length })} onClick=${() => invoke('open_book', {})} />
              </div>`}
            </div>`}
        <hr style="margin: 2px 0" />
        <${InviteRow} model=${model} />`}

    ${model.problem && html`<div class="small danger">${model.problem}</div>`}
  </div>`;
}

/// For an empty book: the own link with a line to send along, to paste into a chat.
function ShareButton({ model }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    invoke('copy', { text: `${INVITE_MESSAGE}\n${model.inviteUrl}` });
    setCopied(true);
    setTimeout(() => setCopied(false), 1800);
  };
  return html`<button class="primary" onClick=${copy}>
    <${Icon} name=${copied ? 'check' : 'share'} />${copied ? tr('Скопировано, отправь в чат') : tr('Скопировать свой Love ID')}
  </button>`;
}

/// A person in the book: a row with a name, a click sends.
function ContactRow({ model, contact }) {
  const [hover, setHover] = useState(false);
  const color = colorFor(contact);
  const sent = model.sent === contact.id;
  return html`<button class="contact-row" onClick=${() => invoke('send', { id: contact.id })}
      onMouseEnter=${() => setHover(true)} onMouseLeave=${() => setHover(false)}
      style=${{ background: sent ? color.tint : hover ? color.tint + '99' : 'transparent' }}>
    <${Avatar} label=${contact.label} color=${color} size=${30} />
    <span class="name grow ellipsis">${contact.label}</span>
    ${sent && html`<span class="sent" style=${{ color: color.deep }}>${tr('Улетело ♥')}</span>`}
  </button>`;
}

// ---- circles ---------------------------------------------------------------------

const ANYONE = '★anyone';

/// The close people: circles without names on top of the menu, after the one
/// for anyone in the world. The name shows under them on hover; a drag puts a
/// circle elsewhere.
function CloseCircles({ model }) {
  const [hovered, setHovered] = useState(null);
  const [dragging, setDragging] = useState(null);
  const now = useNow(10_000);
  const close = closeContacts(model);
  const hover = (id) => ({
    onMouseEnter: () => setHovered(id),
    onMouseLeave: () => setHovered((h) => (h === id ? null : h)),
  });

  const drag = (contact) => (event) => startDrag(event, {
    onStart: () => setDragging(contact.id),
    // Reorders live while the dragged one passes over the others.
    onMove: (e) => {
      const over = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-circle]')?.dataset.circle;
      if (over && over !== contact.id) invoke('move_circle', { id: contact.id, target: over });
    },
    onEnd: () => setDragging(null),
  });

  const add = () => (model.contacts.length === 0
    ? invoke('set_route', { route: { name: 'add', invite: null } })
    : invoke('open_book', {}));
  const addHelp = model.contacts.length === 0 ? tr('Добавить по ссылке') : tr('Добавить кружок');

  // Settings → "Кружки в меню": bare circles (a name under them on hover), or
  // each with its name under it.
  const named = model.settings.circleNames;
  const cell = (circle, name, style) => (named
    ? html`<div class="cell">${circle}<span class="nm ellipsis" style=${style}>${name}</span></div>`
    : circle);
  return html`<div class="col" style="gap: 4px">
    <div class=${'circles' + (named ? ' named' : '')}>
      ${cell(html`<${AnyoneCircle} model=${model} now=${now} lift=${hovered === ANYONE} hover=${hover(ANYONE)} />`, tr('Кому-то'))}
      ${close.map((contact) => {
        const color = colorFor(contact);
        const sent = model.sent === contact.id;
        return cell(html`<button key=${contact.id} data-circle=${contact.id}
            class=${'circle' + (sent || hovered === contact.id ? ' lift' : '') + (dragging === contact.id ? ' dragging' : '')}
            style=${{ background: sent ? color.ring : color.tint, color: sent ? 'var(--paper)' : color.deep, boxShadow: `inset 0 0 0 3px ${color.ring}` }}
            title=${contact.label} aria-label=${contact.label}
            onClick=${() => invoke('send', { id: contact.id })} onPointerDown=${drag(contact)} ...${hover(contact.id)}>
          ${sent ? '♥' : firstLetter(contact.label)}
        </button>`, contact.label, sent ? { color: color.deep } : null);
      })}
      ${cell(html`<button class="circle add" title=${addHelp} aria-label=${addHelp} onClick=${add}><${Icon} name="plus" /></button>`, '')}
    </div>
    ${!named || model.sent != null || hovered === ANYONE
      ? html`<div class="circles-caption ellipsis">${caption(model, close, hovered, now)}</div>` : null}
  </div>`;
}

function caption(model, close, hovered, now) {
  // "Улетело" only right after: later in the hour it would be stale.
  const justSent = model.settings.anyoneSentAt != null && now - model.settings.anyoneSentAt < 120_000;
  if (hovered === ANYONE || (hovered == null && model.sent == null && justSent)) {
    const wait = anyoneWait(model, now);
    const text = justSent ? tr('Улетело кому-то ♥') : wait != null ? tr('Кому-нибудь — через {n} мин', { n: wait }) : tr('Кому-нибудь на свете');
    return html`<span class="muted">${text}</span>`;
  }
  const focus = close.find((c) => c.id === (model.sent ?? hovered));
  if (focus) {
    const text = model.sent === focus.id ? tr('{name} — улетело ♥', { name: focus.label }) : focus.label;
    return html`<span style=${{ color: colorFor(focus).deep }}>${text}</span>`;
  }
  if (close.length === 0 && model.contacts.length > 0) return html`<span class="faint">${tr('Нажми «+», чтобы добавить кружки')}</span>`;
  return ' ';
}

/// "Кому-нибудь на свете", the first circle: one signal an hour to a random
/// person online. The ring fills back up over the hour.
function AnyoneCircle({ model, now, lift, hover }) {
  const wait = anyoneWait(model, now);
  const help = wait != null
    ? tr('Кому-нибудь на свете: снова через {n} мин', { n: wait })
    : tr('Кому-нибудь на свете: сигнал уйдёт случайному человеку, раз в час. Он не узнает, от кого.');
  const r = 23.5;
  const length = 2 * Math.PI * r;
  const progress = wait != null ? (60 - wait) / 60 : 0;
  // Not disabled while waiting: a disabled button shows no tooltip.
  return html`<button class=${'circle anyone' + (lift ? ' lift' : '')} title=${help} aria-label=${help}
      style=${{ color: wait == null ? 'var(--star)' : 'var(--star-off)', opacity: model.anyoneSending ? 0.6 : 1 }}
      onClick=${() => invoke('send_anyone')} ...${hover}>
    <svg class="ring" viewBox="0 0 50 50" aria-hidden="true">
      <circle cx="25" cy="25" r=${r} fill="none" stroke="var(--star)" stroke-width="3" opacity=${wait == null ? 1 : 0.25} />
      ${wait != null && html`<circle cx="25" cy="25" r=${r} fill="none" stroke="var(--star)" stroke-width="3" stroke-linecap="round"
        stroke-dasharray=${`${length * progress} ${length}`} />`}
    </svg>
    <${Icon} name="sparkles" size=${21} />
  </button>`;
}

// ---- sign in ---------------------------------------------------------------------

function SignIn({ model }) {
  // Google, Apple and email all happen on lovearound.me/signin in the browser.
  const open = () => invoke('sign_in_in_browser');
  const error = model.problem && html`<div class="error" role="alert"><${Icon} name="alert" size=${14} /><span>${model.problem}</span></div>`;
  return html`<div class="col" style="gap: 14px">
    <div class="col center" style="gap: 10px; padding-top: 8px; align-items: center">
      <div class="row" style="width: 56px; height: 56px; border-radius: 50%; background: var(--heart-soft); color: var(--heart); justify-content: center">
        <${Icon} name="heart" size=${28} />
      </div>
      <div style="font-size: 17px; font-weight: 700">${tr('Привет!')}</div>
      <div class="muted">${tr('Думаешь о ком-то? Нажми, и он почувствует, что его любят и он нужен. А от кого, не узнает.')}</div>
    </div>

    ${model.waitingForBrowser
      ? html`<div class="col" style="gap: 8px">
          <button class="primary" disabled>${tr('Продолжи в браузере…')}</button>
          <div class="row center" style="gap: 5px; justify-content: center; font-size: 12.5px">
            <span class="muted">${tr('Страница не открылась?')}</span>
            <button class="link" onClick=${open}>${tr('Открыть ещё раз')}</button>
          </div>
          ${error}
        </div>`
      : html`<div class="col" style="gap: 10px">
          <button class="primary" onClick=${open}><${Icon} name="globe" />${tr('Войти через браузер')}</button>
          <div class="chips">
            <span class="chip"><${Icon} name="google" />Google</span>
            <span class="chip"><${Icon} name="apple" />Apple</span>
            <span class="chip"><${Icon} name="mail" />${tr('Почта')}</span>
          </div>
          ${error}
        </div>`}

    <hr />
    <div class="row muted" style="font-size: 12.5px">
      <span class="grow"></span>
      <button onClick=${() => invoke('quit')}>${tr('Закрыть')}</button>
    </div>
  </div>`;
}

quietBrowser();
render(html`<${Root} />`, document.getElementById('app'));
