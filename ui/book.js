// The book: everyone on the left, split into "Рядом" (the circles in the
// menu, in their order) and "Остальные"; the chosen one on the right.
import {
  html, render, useState, useEffect, useRef, invoke, useModel, useDark, quietBrowser, tr,
  Icon, Avatar, Caption, Note, SearchField, LinkRow, SwatchPicker, ContactForm,
  closeContacts, contactsByName, colorFor, colorIndex, palette, contains, peopleCount, startDrag,
} from './lib.js';

function Book() {
  const model = useModel();
  useDark();
  const [adding, setAdding] = useState(false);
  const [query, setQuery] = useState('');
  const shown = useRef(false);

  useEffect(() => {
    if (model && !shown.current) {
      shown.current = true;
      requestAnimationFrame(() => invoke('ready'));
    }
  }, [model]);

  useEffect(() => {
    const key = (e) => {
      if (e.key === 'Escape' && adding) setAdding(false);
    };
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  }, [adding]);

  if (!model) return null;
  const selected = model.contacts.find((c) => c.id === model.bookSelection);
  const select = (id) => {
    setAdding(false);
    invoke('select_contact', { id });
  };

  let detail;
  if (adding) {
    detail = html`<div style="max-width: 380px; padding: 24px 28px">
      <div class="title" style="font-size: 18px; margin-bottom: 14px">${tr('Новый человек')}</div>
      <${ContactForm} model=${model} onAdded=${(id) => select(id)} />
    </div>`;
  } else if (selected) {
    detail = html`<${PersonPage} key=${selected.id} model=${model} contact=${selected} />`;
  } else {
    const empty = model.contacts.length === 0;
    detail = html`<div class="col center" style="height: 100%; justify-content: center; align-items: center; gap: 8px; padding: 30px; text-align: center">
      <div style="font-size: 34px; color: var(--faint)"><${Icon} name="book" /></div>
      <div style="font-size: 15px; font-weight: 600">${empty ? tr('Пока никого нет') : tr('Выбери человека слева')}</div>
      <div class="muted" style="max-width: 280px">${empty ? tr('Добавь людей по их ссылке или поделись своим Love ID.') : tr('Здесь меняются имя и цвет.')}</div>
    </div>`;
  }

  return html`<div class="book">
    <div class="side">
      <div style="padding: 0 10px 4px">
        <${SearchField} value=${query} onInput=${setQuery} placeholder=${tr('Найти — {people}', { people: peopleCount(model.contacts.length) })} />
      </div>
      <${People} model=${model} query=${query.trim()} selected=${adding ? null : model.bookSelection} onSelect=${select} />
      <div style="padding: 6px 12px 12px; border-top: 1px solid var(--hairline)" class="col">
        <${LinkRow} icon="plusCircle" title=${tr('Добавить по ссылке')} onClick=${() => { setAdding(true); invoke('select_contact', { id: null }); }} />
        <${Note} style="padding: 2px 4px 0">${tr('Перетаскивай людей между «Рядом» и «Остальные». Порядок в «Рядом» — порядок кружков в меню.')}<//>
      </div>
    </div>
    <div class="detail">${detail}</div>
  </div>`;
}

/// The two lists with dragging between and within them. A drop is a place:
/// an index among the close ones, or null for "Остальные".
function People({ model, query, selected, onSelect }) {
  const [drag, setDrag] = useState(null); // { id, label, x, y, drop: { closeAt } | null, mark }
  const list = useRef();
  const close = closeContacts(model);
  const others = contactsByName(model).filter((c) => !c.close);

  if (query) {
    const found = contactsByName(model).filter((c) => contains(c.label, query));
    return html`<div class="list">
      ${found.map((c) => html`<${Row} key=${c.id} model=${model} contact=${c} on=${selected === c.id} onClick=${() => onSelect(c.id)} />`)}
      ${found.length === 0 && html`<div class="muted center" style="padding: 14px 0">${tr('Никого с таким именем')}</div>`}
    </div>`;
  }

  /// Where a drop at this point would go, and which line to draw.
  const target = (x, y, dragId) => {
    const node = document.elementFromPoint(x, y);
    const row = node?.closest('[data-row]');
    if (row) {
      const id = row.dataset.row;
      if (id === dragId) return null;
      if (row.dataset.close === '1') {
        const before = close.filter((c) => c.id !== dragId).findIndex((c) => c.id === id);
        const box = row.getBoundingClientRect();
        const after = y > box.top + box.height / 2;
        return { closeAt: before + (after ? 1 : 0), mark: { id, side: after ? 'after' : 'before' } };
      }
      return { closeAt: null, mark: { heading: 'others' } };
    }
    const heading = node?.closest('[data-heading]')?.dataset.heading;
    if (heading === 'close') return { closeAt: close.filter((c) => c.id !== dragId).length, mark: { heading: 'close' } };
    if (heading === 'others') return { closeAt: null, mark: { heading: 'others' } };
    if (node && list.current?.contains(node)) {
      // Empty space under "Рядом" or the end of the list.
      const othersTop = list.current.querySelector('[data-heading=others]')?.getBoundingClientRect().top ?? Infinity;
      return y < othersTop
        ? { closeAt: close.filter((c) => c.id !== dragId).length, mark: { heading: 'close' } }
        : { closeAt: null, mark: { heading: 'others' } };
    }
    return null;
  };

  const startRow = (contact) => (event) => startDrag(event, {
    onStart: (e) => setDrag({ id: contact.id, label: contact.label, x: e.clientX, y: e.clientY, drop: null }),
    onMove: (e) => setDrag((d) => d && { ...d, x: e.clientX, y: e.clientY, drop: target(e.clientX, e.clientY, contact.id) }),
    onEnd: (e) => {
      const drop = target(e.clientX, e.clientY, contact.id);
      setDrag(null);
      if (!drop) return;
      if (drop.closeAt == null && !contact.close) return;
      invoke('place', { id: contact.id, closeAt: drop.closeAt });
    },
  });

  const mark = drag?.drop?.mark;
  const rowMark = (id) => (mark?.id === id ? (mark.side === 'after' ? ' drop-after' : ' drop-before') : '');
  const row = (c) => html`<${Row} key=${c.id} model=${model} contact=${c} on=${selected === c.id}
    extra=${rowMark(c.id) + (drag?.id === c.id ? ' dragging' : '')}
    onClick=${() => onSelect(c.id)} onPointerDown=${startRow(c)} />`;

  return html`<div class="list" ref=${list}>
    <div class=${'book-heading row' + (mark?.heading === 'close' ? ' drop-target' : '')} data-heading="close" style="gap: 6px">
      <span style="color: var(--heart); font-size: 11px"><${Icon} name="heart" /></span>
      <${Caption}>${tr('Рядом')}<//><span class="note">${tr('кружки в меню')}</span>
    </div>
    ${close.map(row)}
    ${close.length === 0 && html`<div class="note" style="padding: 4px 6px 8px">${tr('Перетащи сюда тех, кому хочешь нажимать в один клик')}</div>`}
    <div class=${'book-heading others' + (mark?.heading === 'others' ? ' drop-target' : '')} data-heading="others"><${Caption}>${tr('Остальные')}<//></div>
    ${others.map(row)}
    ${others.length === 0 && model.contacts.length > 0 && html`<div class="note" style="padding: 4px 6px 8px">${tr('Перетащи сюда, чтобы убрать кружок из меню')}</div>`}
    ${drag && html`<div class="ghost" style=${{ left: drag.x + 12 + 'px', top: drag.y - 14 + 'px' }}>
      <${Avatar} label=${drag.label} color=${colorFor(model, model.contacts.find((c) => c.id === drag.id) ?? { id: drag.id })} size=${20} />${drag.label}
    </div>`}
  </div>`;
}

function Row({ model, contact, on, extra = '', onClick, onPointerDown }) {
  return html`<button class=${'book-row' + (on ? ' on' : '') + extra} data-row=${contact.id} data-close=${contact.close ? '1' : '0'}
      onClick=${onClick} onPointerDown=${onPointerDown}>
    <${Avatar} label=${contact.label} color=${colorFor(model, contact)} size=${22} />
    <span class="grow ellipsis">${contact.label}</span>
  </button>`;
}

/// The name (saved a moment after typing stops) and the color.
function PersonPage({ model, contact }) {
  const [name, setName] = useState(contact.label);
  const [color, setColor] = useState(() => colorIndex(model, contact));
  const [status, setStatus] = useState(null); // 'saving' | 'saved' | error text
  const [confirm, setConfirm] = useState(false);
  const timer = useRef();
  const latest = useRef({ name, color });
  latest.current = { name, color };

  const save = async () => {
    clearTimeout(timer.current);
    const label = latest.current.name.trim();
    if (!label || label.length > 64) return;
    setStatus('saving');
    try {
      await invoke('update_contact', { id: contact.id, label, color: latest.current.color });
      setStatus('saved');
    } catch {
      setStatus(tr('Не получилось сохранить'));
    }
  };

  // A change not yet saved goes out when another person is chosen.
  useEffect(() => () => {
    if (timer.current) {
      clearTimeout(timer.current);
      const label = latest.current.name.trim();
      if (label && label.length <= 64) invoke('update_contact', { id: contact.id, label, color: latest.current.color });
    }
  }, []);

  const typed = (value) => {
    setName(value);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => { timer.current = null; save(); }, 700);
  };
  const picked = (i) => {
    setColor(i);
    latest.current = { name, color: i };
    save();
  };
  const remove = async () => {
    try {
      await invoke('remove_contact', { id: contact.id });
    } catch {
      setConfirm(false);
      setStatus(tr('Не получилось удалить'));
    }
  };

  const p = palette(color);
  const who = name.trim() || tr('Человек');
  return html`<div class="col" style="gap: 20px; padding: 24px 28px; max-width: 420px">
    <div class="row" style="gap: 14px">
      <${Avatar} label=${name || contact.label} color=${p} size=${52} />
      <input class="name-input grow" value=${name} maxlength="64" spellcheck="false"
        onInput=${(e) => typed(e.target.value)} onKeyDown=${(e) => e.key === 'Enter' && save()} onBlur=${() => timer.current && save()} />
    </div>

    <div class="col" style="gap: 8px">
      <div class="row"><${Caption}>${tr('Цвет')}<//><span class="grow"></span><span class="small" style=${{ color: p.deep }}>${p.name}</span></div>
      <${SwatchPicker} value=${color} onChange=${picked} size=${32} />
    </div>

    <${Note}>${tr('{name} не увидит ни имя, ни цвет: их видишь только ты.', { name: who })}<//>

    <div class="row" style="gap: 10px; min-height: 28px">
      ${confirm
        ? html`<span class="small">${tr('Удалить из книги?')}</span>
          <button class="pill" onClick=${() => setConfirm(false)}>${tr('Отмена')}</button>
          <button class="pill danger-fill" onClick=${remove}>${tr('Удалить')}</button>`
        : html`<button class="row danger small" style="gap: 6px" onClick=${() => setConfirm(true)}><${Icon} name="trash" />${tr('Удалить из книги')}</button>`}
      <span class="grow"></span>
      <span class=${'small ' + (status === 'saving' || status === 'saved' ? 'faint' : 'danger')}>
        ${status === 'saving' ? tr('Сохраняем…') : status === 'saved' ? tr('Сохранено') : status}
      </span>
    </div>
  </div>`;
}

quietBrowser();
render(html`<${Book} />`, document.getElementById('app'));
