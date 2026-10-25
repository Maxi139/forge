/* @ds-bundle: {"format":4,"namespace":"Forge","components":[{"name":"Icon"},{"name":"Button"},{"name":"Kbd"},{"name":"Badge"},{"name":"StatusDot"},{"name":"Input"},{"name":"Select"},{"name":"Tabs"},{"name":"Panel"},{"name":"Sidebar"},{"name":"StatusBar"},{"name":"TitleBar"},{"name":"CommandPalette"},{"name":"Toast"},{"name":"Dialog"},{"name":"CodeEditor"},{"name":"SerialMonitor"},{"name":"Plotter"},{"name":"BuildProgress"},{"name":"Pinout"},{"name":"MemoryMeter"},{"name":"PartitionMap"},{"name":"LibraryItem"},{"name":"Traces"},{"name":"EmptyState"},{"name":"Splash"}]} */
(function () {
  var R = window.React, h = R.createElement;
  var useState = R.useState, useEffect = R.useEffect, useRef = R.useRef, useMemo = R.useMemo;

  function cx() { var a = []; for (var i = 0; i < arguments.length; i++) if (arguments[i]) a.push(arguments[i]); return a.join(' '); }
  function omit(p, keys) { var o = {}; for (var k in p) if (keys.indexOf(k) < 0) o[k] = p[k]; return o; }
  var uid = 0;
  function useId() { var r = useRef(null); if (r.current == null) r.current = 'fg-' + (++uid); return r.current; }

  /* ---------- Icons: 16px grid, 1.5px stroke, round caps ---------- */
  var ICONS = {
    files: 'M5.75 1.75h4L13 5v7.25a.75.75 0 0 1-.75.75h-6.5a.75.75 0 0 1-.75-.75V2.5a.75.75 0 0 1 .75-.75z M9.5 1.75V5.25H13 M2.75 4.75v8.75a.75.75 0 0 0 .75.75h6',
    file: 'M4 1.75h4.75L12.5 5.5v8a.75.75 0 0 1-.75.75h-7.5a.75.75 0 0 1-.75-.75V2.5a.75.75 0 0 1 .75-.75z M8.5 1.75V5.75h4',
    'file-code': 'M4 1.75h4.75L12.5 5.5v8a.75.75 0 0 1-.75.75h-7.5a.75.75 0 0 1-.75-.75V2.5a.75.75 0 0 1 .75-.75z M8.5 1.75V5.75h4 M6.5 8.5L5.25 10l1.25 1.5 M9.5 8.5l1.25 1.5-1.25 1.5',
    folder: 'M1.75 4.25a1 1 0 0 1 1-1h3.4l1.6 1.6h5.5a1 1 0 0 1 1 1v6.4a1 1 0 0 1-1 1H2.75a1 1 0 0 1-1-1z',
    'folder-open': 'M1.75 12V4.25a1 1 0 0 1 1-1h3.4l1.6 1.6h4.5a1 1 0 0 1 1 1V7 M1.75 12l1.6-4.25a1 1 0 0 1 .95-.65h9.2a.6.6 0 0 1 .56.8L12.6 11.6a1 1 0 0 1-.94.65H1.75',
    search: 'M7 2.25a4.75 4.75 0 1 0 0 9.5 4.75 4.75 0 0 0 0-9.5z M10.5 10.5L14 14',
    library: 'M8 1.75l5.5 3.1v6.3L8 14.25l-5.5-3.1v-6.3z M2.5 4.85L8 8l5.5-3.15 M8 8v6.25 M5.25 3.3l5.5 3.1',
    board: 'M4.5 1.75h7a1 1 0 0 1 1 1v10.5a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1V2.75a1 1 0 0 1 1-1z M5.75 4.25h4.5v3.5h-4.5z M5.75 10.25h0 M5.75 12.25h0 M10.25 10.25h0 M10.25 12.25h0 M8 10.25h0 M8 12.25h0',
    chip: 'M4.5 4.5h7v7h-7z M6.5 1.75v2.75 M9.5 1.75v2.75 M6.5 11.5v2.75 M9.5 11.5v2.75 M1.75 6.5h2.75 M1.75 9.5h2.75 M11.5 6.5h2.75 M11.5 9.5h2.75',
    usb: 'M6 1.75h4v3.5H6z M4.75 5.25h6.5V9a3.25 3.25 0 0 1-6.5 0z M8 12.25v2 M7 2.9v1.2 M9 2.9v1.2',
    build: 'M2.75 8.5l3.25 3.25 7.25-7.5',
    flash: 'M9.25 1.75L3.75 9h4.1l-1.1 5.25L12.25 7H8.1z',
    serial: 'M1.75 8h2.1l1.55-4.25 2.1 8.5 2.1-6.5 1.4 2.25h3.25',
    plot: 'M2.25 2v11.75H14 M4.5 10.5l2.75-3.75 2.5 2 3.75-5',
    device: 'M4.75 1.75h6.5a1 1 0 0 1 1 1v10.5a1 1 0 0 1-1 1h-6.5a1 1 0 0 1-1-1V2.75a1 1 0 0 1 1-1z M6 4h0 M6 6.5h0 M6 9h0 M6 11.5h0 M10 4h0 M10 6.5h0 M10 9h0 M10 11.5h0',
    settings: 'M2 4.5h6.5 M11.5 4.5H14 M2 11.5h2.5 M7.5 11.5H14 M10 2.75v3.5 M6 9.75v3.5',
    command: 'M2.75 4.25L6.5 8l-3.75 3.75 M8.5 11.75h4.75',
    play: 'M4.75 2.75v10.5L13 8z',
    stop: 'M4.25 4.25h7.5v7.5h-7.5z',
    check: 'M3.25 8.5l3 3 6.5-7',
    'check-circle': 'M8 1.75a6.25 6.25 0 1 0 0 12.5 6.25 6.25 0 0 0 0-12.5z M5.25 8.25l1.9 1.9 3.6-3.9',
    warning: 'M8 2.25l6 10.75H2z M8 6.5v3 M8 11.25h0',
    error: 'M8 1.75a6.25 6.25 0 1 0 0 12.5 6.25 6.25 0 0 0 0-12.5z M5.9 5.9l4.2 4.2 M10.1 5.9l-4.2 4.2',
    info: 'M8 1.75a6.25 6.25 0 1 0 0 12.5 6.25 6.25 0 0 0 0-12.5z M8 7.25v4 M8 4.85h0',
    close: 'M4 4l8 8 M12 4l-8 8',
    'chevron-down': 'M4 6l4 4 4-4',
    'chevron-right': 'M6 4l4 4-4 4',
    plus: 'M8 3v10 M3 8h10',
    more: 'M3.5 8h0 M8 8h0 M12.5 8h0',
    refresh: 'M13.25 8a5.25 5.25 0 1 1-1.55-3.7 M13.25 2.25v2.75H10.5',
    split: 'M2.25 2.75h11.5v10.5H2.25z M8 2.75v10.5',
    trash: 'M2.75 4.25h10.5 M6.25 4.25V2.75h3.5v1.5 M4 4.25l.6 9a1 1 0 0 0 1 .95h4.8a1 1 0 0 0 1-.95l.6-9',
    wifi: 'M1.75 6.25a8.75 8.75 0 0 1 12.5 0 M4 8.75a5.5 5.5 0 0 1 8 0 M6.25 11.1a2.3 2.3 0 0 1 3.5 0 M8 13.5h0',
    bug: 'M5.5 6h5v4.25a2.5 2.5 0 0 1-5 0z M6.25 6a1.75 1.75 0 0 1 3.5 0 M2.75 8.5H5.5 M10.5 8.5h2.75 M3.25 12.5l2.4-1.2 M12.75 12.5l-2.4-1.2 M3.25 4.5l2.25 1.5 M12.75 4.5L10.5 6',
    spark: 'M8 1.75c.45 3.2 1.6 4.35 5 5-3.4.65-4.55 1.8-5 5-.45-3.2-1.6-4.35-5-5 3.4-.65 4.55-1.8 5-5z M13 11.25v3 M11.5 12.75h3',
    partition: 'M2.25 3.25h11.5v9.5H2.25z M5.75 3.25v9.5 M10 3.25v9.5',
    memory: 'M1.75 5h12.5v5.5H1.75z M4.25 10.5v2 M6.75 10.5v2 M9.25 10.5v2 M11.75 10.5v2 M4.5 7.75h2 M9.5 7.75h2',
    erase: 'M6.25 13.25h7.5 M2.6 9.4l5.9-5.9a1.25 1.25 0 0 1 1.75 0l2.25 2.25a1.25 1.25 0 0 1 0 1.75L8 12.05a1.25 1.25 0 0 1-1.75 0L2.6 11.15a1.25 1.25 0 0 1 0-1.75z M5.25 6.75l4 4',
    pause: 'M5.25 3.25v9.5 M10.75 3.25v9.5',
    clock: 'M8 1.75a6.25 6.25 0 1 0 0 12.5 6.25 6.25 0 0 0 0-12.5z M8 4.5V8l2.5 1.5',
    'arrow-down': 'M8 2.75v10.5 M3.75 9L8 13.25 12.25 9',
    send: 'M2.25 8h8.5 M7.5 4.5L11 8l-3.5 3.5 M13.75 3v10',
    branch: 'M4.5 2.25v11.5 M4.5 9.75c0-3 7-2.5 7-5.5 M11.5 2.25a2 2 0 1 0 0 4 2 2 0 0 0 0-4z'
  };

  function Icon(p) {
    var s = p.size || 16;
    return h('svg', {
      className: cx('fg-icon', p.className), width: s, height: s, viewBox: '0 0 16 16', fill: 'none', stroke: 'currentColor',
      strokeWidth: p.strokeWidth || (p.name === 'more' ? 2 : 1.5), strokeLinecap: 'round', strokeLinejoin: 'round',
      role: p.label ? 'img' : undefined, 'aria-label': p.label, 'aria-hidden': p.label ? undefined : true, style: p.style
    }, h('path', { d: ICONS[p.name] || ICONS.more }));
  }
  Icon.names = Object.keys(ICONS);

  /* ---------- Button, Kbd, Badge, StatusDot ---------- */
  function Button(p) {
    var v = p.variant || 'secondary', s = p.size || 'md';
    var o = omit(p, ['variant', 'size', 'icon', 'iconRight', 'busy', 'children', 'className', 'shortcut']);
    var iconOnly = p.children == null && p.icon;
    return h('button', Object.assign({ type: 'button' }, o, {
      className: cx('fg-btn', 'fg-btn--' + v, 'fg-btn--' + s, iconOnly && 'fg-btn--icon', p.busy && 'is-busy', p.className),
      'aria-busy': p.busy || undefined
    }),
      p.icon ? h(Icon, { name: p.icon, size: s === 'lg' ? 16 : 14 }) : null,
      p.children != null ? h('span', null, p.children) : null,
      p.shortcut ? h('span', { className: 'fg-btn-kbd' }, p.shortcut) : null,
      p.iconRight ? h(Icon, { name: p.iconRight, size: 12 }) : null);
  }

  function Kbd(p) {
    var keys = p.keys || [p.children];
    return h('span', { className: cx('fg-kbds', p.className) }, keys.map(function (k, i) { return h('kbd', { key: i, className: 'fg-kbd' }, k); }));
  }

  function Badge(p) {
    return h('span', { className: cx('fg-badge', 'fg-badge--' + (p.tone || 'neutral'), p.mono && 'fg-badge--mono', p.className) },
      p.icon ? h(Icon, { name: p.icon, size: 12 }) : null, p.children);
  }

  var STATUS_LABEL = { idle: 'Nicht verbunden', scanning: 'Suche Port …', connected: 'Verbunden', compiling: 'Kompiliere …', flashing: 'Flashe …', error: 'Verbindung verloren' };
  function StatusDot(p) {
    var st = p.state || 'idle';
    return h('span', { className: cx('fg-status', 'fg-status--' + st, p.className), role: 'status' },
      h('span', { key: st, className: 'fg-dot' }),
      p.label === false ? null : h('span', null, p.label || STATUS_LABEL[st]));
  }

  /* ---------- Input, Select ---------- */
  function Input(p) {
    var id = useId();
    var o = omit(p, ['label', 'hint', 'error', 'icon', 'className', 'size', 'mono', 'trailing']);
    return h('div', { className: cx('fg-field', p.error && 'is-invalid', p.className) },
      p.label ? h('label', { className: 'fg-field-label', htmlFor: id }, p.label) : null,
      h('span', { className: cx('fg-input', p.size === 'sm' && 'fg-input--sm', p.mono && 'fg-input--mono') },
        p.icon ? h(Icon, { name: p.icon, size: 14 }) : null,
        h('input', Object.assign({ id: id, 'aria-invalid': p.error ? true : undefined }, o)),
        p.trailing || null),
      p.error || p.hint ? h('span', { className: p.error ? 'fg-field-error' : 'fg-field-hint' }, p.error || p.hint) : null);
  }

  function Select(p) {
    var id = useId();
    var o = omit(p, ['label', 'hint', 'options', 'icon', 'className', 'size', 'mono']);
    var opts = (p.options || []).map(function (x) { return typeof x === 'string' ? { value: x, label: x } : x; });
    return h('div', { className: cx('fg-field', p.className) },
      p.label ? h('label', { className: 'fg-field-label', htmlFor: id }, p.label) : null,
      h('span', { className: cx('fg-input', 'fg-input--select', p.size === 'sm' && 'fg-input--sm', p.mono && 'fg-input--mono') },
        p.icon ? h(Icon, { name: p.icon, size: 14 }) : null,
        h('select', Object.assign({ id: id }, o), opts.map(function (x) { return h('option', { key: x.value, value: x.value }, x.label); })),
        h(Icon, { name: 'chevron-down', size: 12, className: 'fg-select-chev' })),
      p.hint ? h('span', { className: 'fg-field-hint' }, p.hint) : null);
  }

  /* ---------- Tabs, Panel ---------- */
  function Tabs(p) {
    var v = p.variant || 'editor';
    var first = p.items[0] && p.items[0].id;
    var st = useState(p.defaultValue != null ? p.defaultValue : first);
    var val = p.value != null ? p.value : st[0];
    function sel(id) { if (p.value == null) st[1](id); if (p.onChange) p.onChange(id); }
    function key(e, i) {
      var d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
      if (!d) return;
      var n = p.items[(i + d + p.items.length) % p.items.length];
      sel(n.id);
      var next = e.currentTarget.parentNode.children[(i + d + p.items.length) % p.items.length];
      if (next) next.focus();
    }
    return h('div', { className: cx('fg-tabs', 'fg-tabs--' + v, p.className), role: 'tablist' },
      p.items.map(function (t, i) {
        var a = t.id === val;
        return h('div', {
          key: t.id, role: 'tab', 'aria-selected': a, tabIndex: a ? 0 : -1, title: t.title,
          className: cx('fg-tab', a && 'is-active', t.dirty && 'is-dirty', t.problems && 'has-problems'),
          onClick: function () { sel(t.id); }, onKeyDown: function (e) { key(e, i); }
        },
          t.icon ? h(Icon, { name: t.icon, size: 14 }) : null,
          h('span', { className: 'fg-tab-label' }, t.label),
          t.count != null ? h('span', { className: 'fg-tab-count' }, t.count) : null,
          v === 'editor' ? h('button', {
            className: 'fg-tab-close', tabIndex: -1, 'aria-label': t.label + ' schließen',
            onClick: function (e) { e.stopPropagation(); if (p.onClose) p.onClose(t.id); }
          }, h('span', { className: 'fg-tab-dirty' }), h(Icon, { name: 'close', size: 12 })) : null);
      }),
      p.actions ? h('div', { className: 'fg-tabs-actions' }, p.actions) : null);
  }

  function Panel(p) {
    var head = p.title != null || p.actions || p.tabs;
    return h('section', { className: cx('fg-panel', p.className), style: p.style, 'aria-label': typeof p.title === 'string' ? p.title : undefined },
      head ? h('header', { className: 'fg-panel-head' }, p.tabs || h('span', { className: 'fg-overline' }, p.title), h('div', { className: 'fg-panel-actions' }, p.actions)) : null,
      h('div', { className: cx('fg-panel-body', p.padded && 'is-padded') }, p.children));
  }

  /* ---------- Sidebar: ActivityBar + FileTree ---------- */
  function ActivityBar(p) {
    function item(it) {
      return h('button', {
        key: it.id, className: cx('fg-act', it.id === p.value && 'is-active'), title: it.label, 'aria-label': it.label,
        'aria-pressed': it.id === p.value, onClick: function () { if (p.onChange) p.onChange(it.id); }
      }, h(Icon, { name: it.icon, size: 18 }), it.badge ? h('span', { className: 'fg-act-badge' }, it.badge) : null);
    }
    return h('nav', { className: 'fg-activity', 'aria-label': 'Ansichten' },
      (p.items || []).map(item), h('div', { className: 'fg-activity-spacer' }), (p.bottom || []).map(item));
  }

  function FileTree(p) {
    var init = useMemo(function () {
      var m = {};
      (function walk(ns, base) { ns.forEach(function (n) { var path = base + '/' + n.name; if (n.open) m[path] = true; if (n.children) walk(n.children, path); }); })(p.nodes || [], '');
      return m;
    }, []);
    var o = useState(init), s = useState(p.defaultSelected);
    var open = o[0], sel = p.selected !== undefined ? p.selected : s[0];
    function rows(ns, depth, base) {
      return ns.reduce(function (acc, n) {
        var path = base + '/' + n.name, isF = !!n.children, isOpen = !!open[path];
        function act() {
          if (isF) { var m = Object.assign({}, open); m[path] = !isOpen; o[1](m); }
          else { s[1](path); if (p.onSelect) p.onSelect(path, n); }
        }
        acc.push(h('div', {
          key: path, role: 'treeitem', tabIndex: 0, 'aria-expanded': isF ? isOpen : undefined, 'aria-selected': sel === path,
          className: cx('fg-tree-row', sel === path && 'is-selected', n.status && 'is-' + n.status),
          style: { paddingLeft: 8 + depth * 12 }, onClick: act,
          onKeyDown: function (e) { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); act(); } }
        },
          h('span', { className: 'fg-tree-chev' }, isF ? h(Icon, { name: isOpen ? 'chevron-down' : 'chevron-right', size: 12 }) : null),
          h(Icon, { name: isF ? (isOpen ? 'folder-open' : 'folder') : (n.icon || 'file'), size: 14, className: 'fg-tree-icon' }),
          h('span', { className: 'fg-tree-name' }, n.name),
          n.badge != null ? h('span', { className: 'fg-tree-badge' }, n.badge) : null));
        if (isF && isOpen) acc.push.apply(acc, rows(n.children, depth + 1, path));
        return acc;
      }, []);
    }
    return h('div', { className: 'fg-tree', role: 'tree' }, rows(p.nodes || [], 0, ''));
  }

  var DEFAULT_ACTIVITY = [
    { id: 'files', icon: 'files', label: 'Explorer' }, { id: 'search', icon: 'search', label: 'Suche' },
    { id: 'library', icon: 'library', label: 'Bibliotheken' }, { id: 'device', icon: 'device', label: 'Gerät' },
    { id: 'plot', icon: 'plot', label: 'Plotter' }
  ];
  function Sidebar(p) {
    var a = useState(p.defaultView || 'files');
    var view = p.view != null ? p.view : a[0];
    return h('div', { className: cx('fg-sidebar', p.className) },
      h(ActivityBar, { items: p.activity || DEFAULT_ACTIVITY, bottom: p.activityBottom || [{ id: 'settings', icon: 'settings', label: 'Einstellungen' }], value: view, onChange: function (id) { a[1](id); if (p.onViewChange) p.onViewChange(id); } }),
      p.collapsed ? null : h(Panel, { title: p.title || 'Explorer', actions: p.actions }, p.children || (p.nodes ? h(FileTree, { nodes: p.nodes, defaultSelected: p.defaultSelected, onSelect: p.onSelect }) : null)));
  }

  /* ---------- StatusBar, TitleBar ---------- */
  function StatusItem(p) {
    return h(p.onClick ? 'button' : 'span', { className: cx('fg-status-item', p.className), onClick: p.onClick, title: p.title },
      p.icon ? h(Icon, { name: p.icon, size: 12 }) : null, p.children);
  }
  function StatusBar(p) {
    return h('footer', { className: cx('fg-statusbar', p.className) },
      p.progress != null ? h('span', { className: 'fg-statusbar-progress', style: { width: Math.round(p.progress * 100) + '%' } }) : null,
      h('div', { className: 'fg-statusbar-group' }, p.left),
      h('div', { className: 'fg-statusbar-group' }, p.right));
  }

  function TitleBar(p) {
    var phase = p.phase || 'idle';
    return h('header', { className: cx('fg-titlebar', p.className) },
      p.windowControls === false ? null : h('span', { className: 'fg-titlebar-lights', 'aria-hidden': true }, h('i'), h('i'), h('i')),
      h('span', { className: 'fg-wordmark' }, h('b'), 'forge'),
      h('span', { className: 'fg-titlebar-project' }, '/', h('strong', null, p.project || 'sketch')),
      h('div', { className: 'fg-titlebar-center' },
        h('button', { className: 'fg-titlebar-search', onClick: p.onSearch, type: 'button' }, h(Icon, { name: 'search', size: 14 }), h('span', null, p.searchLabel || 'Befehle, Dateien, Boards …'), h(Kbd, { keys: ['⌘', 'K'] }))),
      h('div', { className: 'fg-titlebar-group' },
        h('div', { className: 'fg-target' },
          h('button', { type: 'button', onClick: p.onBoard, title: 'Board wählen' }, h(Icon, { name: 'chip', size: 14 }), p.board || 'ESP32-S3', h(Icon, { name: 'chevron-down', size: 12 })),
          h('button', { type: 'button', onClick: p.onPort, title: 'Port wählen' }, h(StatusDot, { state: p.connection || 'connected', label: false }), h('span', { className: 'fg-mono' }, p.port || 'usbmodem1101'))),
        h(Button, { variant: 'ghost', icon: 'build', 'aria-label': 'Kompilieren', title: 'Kompilieren  ⌘B', busy: phase === 'compiling', onClick: p.onBuild }),
        h(Button, { variant: 'primary', icon: 'flash', busy: phase === 'flashing', onClick: p.onFlash, shortcut: phase === 'flashing' ? null : '⌘U' }, phase === 'flashing' ? 'Flashe …' : 'Flash')));
  }

  /* ---------- CommandPalette ---------- */
  function fuzzy(q, s) {
    if (!q) return { score: 0, idx: [] };
    var ql = q.toLowerCase(), sl = s.toLowerCase(), idx = [], j = 0, score = 0, prev = -2;
    for (var i = 0; i < sl.length && j < ql.length; i++) {
      if (sl[i] === ql[j]) { idx.push(i); score += (i === prev + 1 ? 3 : 1) + (i === 0 || sl[i - 1] === ' ' ? 2 : 0); prev = i; j++; }
    }
    return j === ql.length ? { score: score, idx: idx } : null;
  }
  function marks(s, idx) {
    if (!idx || !idx.length) return s;
    var out = [], last = 0;
    idx.forEach(function (i, k) { if (i > last) out.push(s.slice(last, i)); out.push(h('mark', { key: k }, s[i])); last = i + 1; });
    out.push(s.slice(last));
    return out;
  }
  function CommandPalette(p) {
    var qs = useState(p.defaultQuery || ''), as = useState(0);
    var q = qs[0], a = as[0];
    var list = useMemo(function () {
      return (p.items || []).map(function (it) { var m = fuzzy(q, it.label); return m && { it: it, m: m }; })
        .filter(Boolean).sort(function (x, y) { return q ? y.m.score - x.m.score : 0; });
    }, [q, p.items]);
    var groups = [];
    list.forEach(function (r, i) {
      r.i = i;
      var g = q ? 'Treffer' : (r.it.group || '');
      var last = groups[groups.length - 1];
      if (!last || last.name !== g) groups.push({ name: g, rows: [r] }); else last.rows.push(r);
    });
    function run(r) { if (r && p.onRun) p.onRun(r.it); }
    function key(e) {
      if (e.key === 'ArrowDown') { e.preventDefault(); as[1](Math.min(a + 1, list.length - 1)); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); as[1](Math.max(a - 1, 0)); }
      else if (e.key === 'Enter') run(list[a]);
      else if (e.key === 'Escape' && p.onClose) p.onClose();
    }
    return h('div', { className: cx('fg-palette', p.className), role: 'dialog', 'aria-label': 'Kommandopalette' },
      h('div', { className: 'fg-palette-input' },
        h(Icon, { name: 'command' }),
        h('input', { autoFocus: p.autoFocus, value: q, placeholder: p.placeholder || 'Befehl, Datei oder Board suchen …', 'aria-label': 'Suche', onChange: function (e) { qs[1](e.target.value); as[1](0); }, onKeyDown: key }),
        h(Kbd, { keys: ['esc'] })),
      h('div', { className: 'fg-palette-list', role: 'listbox' },
        list.length ? groups.map(function (g) {
          return h('div', { key: g.name || '_' },
            g.name ? h('div', { className: 'fg-palette-glabel' }, g.name) : null,
            g.rows.map(function (r) {
              var on = r.i === a;
              return h('div', {
                key: r.it.id, role: 'option', 'aria-selected': on, className: cx('fg-palette-item', on && 'is-active'),
                onMouseMove: function () { if (!on) as[1](r.i); }, onClick: function () { run(r); }
              },
                h(Icon, { name: r.it.icon || 'chevron-right' }),
                h('span', { className: 'fg-palette-label' }, marks(r.it.label, r.m.idx)),
                r.it.hint ? h('span', { className: 'fg-palette-hint' }, r.it.hint) : null,
                r.it.shortcut ? h(Kbd, { keys: r.it.shortcut }) : null);
            }));
        }) : h('div', { className: 'fg-palette-empty' }, 'Nichts gefunden für „' + q + '“')),
      h('div', { className: 'fg-palette-foot' },
        h('span', null, h(Kbd, { keys: ['↑', '↓'] }), 'auswählen'),
        h('span', null, h(Kbd, { keys: ['↵'] }), 'ausführen'),
        h('span', null, h(Kbd, { keys: ['>'] }), 'nur Befehle')));
  }

  /* ---------- Toast, Dialog ---------- */
  var KIND_ICON = { success: 'check-circle', error: 'error', warning: 'warning', info: 'info', progress: 'flash' };
  function Toast(p) {
    var k = p.kind || 'info';
    return h('div', { className: cx('fg-toast', 'fg-toast--' + k, p.className), role: k === 'error' ? 'alert' : 'status' },
      h('span', { className: 'fg-toast-icon' }, h(Icon, { name: p.icon || KIND_ICON[k] })),
      h('div', null,
        h('div', { className: 'fg-toast-title' }, p.title),
        p.message ? h('div', { className: 'fg-toast-msg' }, p.message) : null,
        p.actions ? h('div', { className: 'fg-toast-actions' }, p.actions) : null),
      p.onClose ? h('button', { className: 'fg-toast-close', 'aria-label': 'Schließen', onClick: p.onClose }, h(Icon, { name: 'close', size: 12 })) : h('span'),
      p.progress != null ? h('span', { className: 'fg-toast-progress', style: { width: (p.progress * 100) + '%' } }) : null);
  }
  function ToastStack(p) { return h('div', { className: 'fg-toasts', 'aria-live': 'polite' }, p.children); }

  function Dialog(p) {
    var tid = useId();
    useEffect(function () {
      if (p.inline || !p.onClose) return;
      function k(e) { if (e.key === 'Escape') p.onClose(); }
      window.addEventListener('keydown', k);
      return function () { window.removeEventListener('keydown', k); };
    }, [p.inline, p.onClose]);
    var box = h('div', { className: cx('fg-dialog', p.className), role: 'dialog', 'aria-modal': p.inline ? undefined : true, 'aria-labelledby': tid, style: p.width ? { width: p.width } : null },
      h('div', { className: 'fg-dialog-head' },
        p.icon ? h('span', { className: cx('fg-dialog-icon', p.tone && 'is-' + p.tone) }, h(Icon, { name: p.icon })) : null,
        h('h2', { className: 'fg-dialog-title', id: tid }, p.title)),
      h('div', { className: 'fg-dialog-body' }, p.children),
      p.footer ? h('div', { className: 'fg-dialog-foot' }, p.footer) : null);
    return p.inline ? box : h('div', { className: 'fg-scrim', onMouseDown: function (e) { if (e.target === e.currentTarget && p.onClose) p.onClose(); } }, box);
  }

  /* ---------- CodeEditor (visual reference for the editor theme) ---------- */
  var KW = {};
  ('void int float double char bool long short unsigned signed const static volatile return if else for while do switch case break continue default struct class enum public private protected new delete true false nullptr auto using namespace template typename sizeof uint8_t uint16_t uint32_t uint64_t int8_t int16_t int32_t int64_t size_t byte boolean word').split(' ').forEach(function (k) { KW[k] = 1; });
  var TYPE_RE = /^(String|Serial\d?|Wire|SPI|WiFi|WiFiClient|WebServer|Preferences|HardwareSerial|IPAddress|[A-Z][a-z0-9]+(?:_[A-Za-z0-9]+|[A-Z][a-z0-9]+)+)$/;
  function lex(line) {
    var out = [], m, rest = line;
    var pre = /^(\s*)(#\s*\w+)(.*)$/.exec(line);
    if (pre) {
      if (pre[1]) out.push({ t: pre[1] });
      out.push({ t: pre[2], c: 'macro' });
      rest = pre[3];
      var inc = /^(\s*)(<[^>]*>)(.*)$/.exec(rest);
      if (inc) { if (inc[1]) out.push({ t: inc[1] }); out.push({ t: inc[2], c: 'string' }); rest = inc[3]; }
    }
    var re = /(\/\/.*$)|(\/\*.*?\*\/)|("(?:\\.|[^"\\])*"?|'(?:\\.|[^'\\])*'?)|(\b0x[0-9A-Fa-f]+\b|\b\d+(?:\.\d+)?f?\b)|([A-Za-z_]\w*)|(\s+)|([^\sA-Za-z_0-9"']+)/g;
    while ((m = re.exec(rest))) {
      var t = m[0], c;
      if (m[1] || m[2]) c = 'comment';
      else if (m[3]) c = 'string';
      else if (m[4]) c = 'number';
      else if (m[5]) {
        if (KW[t]) c = 'keyword';
        else if (/^[A-Z][A-Z0-9_]+$/.test(t)) c = 'constant';
        else if (/^\s*\(/.test(rest.slice(re.lastIndex))) c = 'function';
        else if (TYPE_RE.test(t)) c = 'type';
      }
      else if (m[7]) c = 'punct';
      out.push({ t: t, c: c });
    }
    return out;
  }
  function renderCode(text, mk, completion) {
    var toks = lex(text), pos = 0, out = [], cuts = [], n = 0;
    if (mk.sq) cuts.push(mk.sq[0], mk.sq[1]);
    if (mk.caret != null) cuts.push(mk.caret);
    function caret() {
      return h('span', { key: 'caret', className: 'fg-caret-at' }, h('span', { className: 'fg-caret' }), completion ? h(Completion, completion) : null);
    }
    toks.forEach(function (tk) {
      var s = pos, e = pos + tk.t.length, pts = [s];
      pos = e;
      cuts.forEach(function (c) { if (c > s && c < e) pts.push(c); });
      pts.sort(function (x, y) { return x - y; });
      pts.push(e);
      for (var i = 0; i < pts.length - 1; i++) {
        var a = pts[i], b = pts[i + 1];
        if (mk.caret === a) out.push(caret());
        var inSq = mk.sq && a >= mk.sq[0] && b <= mk.sq[1];
        out.push(h('span', { key: n++, className: cx(tk.c && 'tk-' + tk.c, inSq && 'fg-sq fg-sq--' + mk.sq[2]) || undefined }, tk.t.slice(a - s, b - s)));
      }
    });
    if (mk.caret != null && mk.caret >= pos) out.push(caret());
    return out;
  }
  function Completion(c) {
    var q = c.query || '';
    return h('div', { className: 'fg-complete', role: 'listbox', contentEditable: false },
      h('div', { className: 'fg-complete-list' }, (c.items || []).map(function (it, i) {
        var m = fuzzy(q, it.label);
        return h('div', { key: it.label, role: 'option', 'aria-selected': i === (c.active || 0), className: cx('fg-complete-item', i === (c.active || 0) && 'is-active') },
          h('span', { className: 'fg-complete-kind k-' + (it.kind || 'v') }, it.kind || 'v'),
          h('span', { className: 'fg-complete-label' }, marks(it.label, m && m.idx)),
          it.detail ? h('span', { className: 'fg-complete-detail' }, it.detail) : null);
      })),
      c.doc ? h('div', { className: 'fg-complete-doc' }, c.doc) : null);
  }
  function CodeEditor(p) {
    var lines = (p.code || '').replace(/\n$/, '').split('\n');
    var diags = {};
    (p.diagnostics || []).forEach(function (d) { diags[d.line] = d; });
    var cur = p.cursor || {};
    return h('div', { className: cx('fg-editor-wrap', p.className), style: p.style },
      p.breadcrumbs ? h('div', { className: 'fg-crumbs' }, p.breadcrumbs.reduce(function (acc, b, i) {
        if (i) acc.push(h(Icon, { key: 'c' + i, name: 'chevron-right', size: 12 }));
        acc.push(h('span', { key: i }, b));
        return acc;
      }, [])) : null,
      h('div', { className: 'fg-editor', role: 'textbox', 'aria-multiline': true, 'aria-readonly': true, 'aria-label': p.label || 'Code', style: p.height ? { height: p.height } : null },
        lines.map(function (text, i) {
          var ln = i + 1, d = diags[ln], active = cur.line === ln;
          var mk = { caret: active ? cur.col : null, sq: d ? [d.from, d.to, d.severity || 'error'] : null };
          return h('div', { key: ln, className: cx('fg-line', active && 'is-active') },
            h('span', { className: 'fg-gutter' }, d ? h('span', { className: 'is-' + (d.severity || 'error') }, h(Icon, { name: d.severity === 'warning' ? 'warning' : 'error', size: 14 })) : null, ln),
            h('span', { className: 'fg-code' }, renderCode(text, mk, active && p.completion ? p.completion : null),
              d ? h('span', { className: 'fg-diag fg-diag--' + (d.severity || 'error') }, h(Icon, { name: d.severity === 'warning' ? 'warning' : 'error', size: 12 }), d.message) : null));
        })));
  }

  /* ---------- SerialMonitor ---------- */
  function fmtTs(ms) { var s = Math.floor(ms / 1000), r = ms % 1000; var mm = Math.floor(s / 60), ss = s % 60; return (mm < 10 ? '0' : '') + mm + ':' + (ss < 10 ? '0' : '') + ss + '.' + ('00' + r).slice(-3); }
  function kv(text) {
    var parts = [], re = /(\w+)(:)(-?\d+(?:\.\d+)?)/g, last = 0, m, n = 0;
    while ((m = re.exec(text))) { if (m.index > last) parts.push(text.slice(last, m.index)); parts.push(h('span', { key: n++, className: 'k' }, m[1] + m[2])); parts.push(m[3]); last = re.lastIndex; }
    parts.push(text.slice(last));
    return parts;
  }
  function Toggle(p) {
    return h('button', { type: 'button', className: cx('fg-toggle', p.icon && 'fg-toggle--icon'), 'aria-pressed': !!p.on, onClick: p.onToggle, title: p.title, 'aria-label': p.icon ? p.title : undefined },
      p.icon ? h(Icon, { name: p.icon, size: 14 }) : h('i'), p.icon ? null : p.children);
  }
  function SerialMonitor(p) {
    var inS = useState(''), tsS = useState(p.timestamps !== false), asS = useState(true);
    var out = useRef(null);
    var lines = p.lines || [];
    useEffect(function () { if (asS[0] && out.current) out.current.scrollTop = out.current.scrollHeight; }, [lines.length, asS[0]]);
    function send(e) {
      if (e.key !== 'Enter' || !inS[0]) return;
      if (p.onSend) p.onSend(inS[0]);
      inS[1]('');
    }
    return h('div', { className: cx('fg-monitor', p.className), style: p.height ? { height: p.height } : null },
      h('div', { className: 'fg-mon-bar' },
        h(StatusDot, { state: p.state || 'connected' }),
        h('span', { className: 'fg-mon-port' }, p.port || '/dev/cu.usbmodem1101'),
        h('span', { className: 'fg-spacer' }),
        h(Toggle, { icon: 'clock', title: 'Zeitstempel', on: tsS[0], onToggle: function () { tsS[1](!tsS[0]); } }),
        h(Toggle, { icon: 'arrow-down', title: 'Autoscroll', on: asS[0], onToggle: function () { asS[1](!asS[0]); } }),
        h(Select, { size: 'sm', mono: true, 'aria-label': 'Baudrate', value: String(p.baud || 115200), onChange: function (e) { if (p.onBaud) p.onBaud(+e.target.value); }, options: ['9600', '57600', '115200', '230400', '460800', '921600'] }),
        h(Button, { variant: 'ghost', size: 'sm', icon: 'trash', 'aria-label': 'Ausgabe leeren', title: 'Ausgabe leeren', onClick: p.onClear })),
      h('div', { className: 'fg-mon-out', ref: out, role: 'log', 'aria-live': 'off' },
        lines.map(function (l, i) {
          var kind = l.kind || 'rx';
          return h('div', { key: l.id != null ? l.id : i, className: 'fg-mon-line fg-mon-line--' + kind },
            tsS[0] && l.t != null ? h('span', { className: 'fg-mon-ts' }, fmtTs(l.t)) : null,
            kind === 'tx' ? h('span', { className: 'fg-mon-dir' }, '→') : null,
            h('span', { className: 'fg-mon-text' }, kind === 'rx' ? kv(l.text) : l.text));
        })),
      h('div', { className: 'fg-mon-in' },
        h('span', { className: 'fg-mon-prompt' }, '›'),
        h('input', { value: inS[0], onChange: function (e) { inS[1](e.target.value); }, onKeyDown: send, placeholder: 'An das Board senden …', 'aria-label': 'Nachricht senden' }),
        h(Select, { size: 'sm', 'aria-label': 'Zeilenende', defaultValue: 'nl', options: [{ value: 'none', label: 'Kein Zeilenende' }, { value: 'nl', label: 'NL' }, { value: 'nlcr', label: 'NL & CR' }] }),
        h(Kbd, { keys: ['↵'] })));
  }

  /* ---------- Plotter ---------- */
  function Plotter(p) {
    var series = p.series || [], n = p.window || 160, H = 100, W = 100;
    var all = []; series.forEach(function (s) { all = all.concat(s.values.slice(-n)); });
    var min = p.min != null ? p.min : Math.min.apply(null, all.concat([0]));
    var max = p.max != null ? p.max : Math.max.apply(null, all.concat([1]));
    if (max === min) max = min + 1;
    function y(v) { return H - (v - min) / (max - min) * H; }
    var ticks = [0, .25, .5, .75, 1].map(function (f) { return min + (max - min) * f; });
    function fmt(v) { var a = Math.abs(max - min); return a >= 100 ? Math.round(v) : a >= 10 ? v.toFixed(1) : v.toFixed(2); }
    return h('div', { className: cx('fg-plot', p.paused && 'is-paused', p.className), style: p.height ? { height: p.height } : null },
      h('div', { className: 'fg-plot-head' },
        series.map(function (s, i) {
          var last = s.values[s.values.length - 1];
          return h('span', { key: s.name, className: 'fg-legend' }, h('i', { className: 'fg-sw fg-sw-' + (i + 1) }), s.name, h('span', { className: 'fg-mono' }, last == null ? '–' : fmt(last) + (s.unit ? ' ' + s.unit : '')));
        }),
        h('span', { className: 'fg-spacer' }),
        p.actions || null,
        p.onPause ? h(Button, { variant: 'ghost', size: 'sm', icon: p.paused ? 'play' : 'pause', onClick: p.onPause }, p.paused ? 'Weiter' : 'Pause') : null),
      h('div', { className: 'fg-plot-area' },
        ticks.map(function (t, i) { return h('span', { key: i, className: 'fg-plot-y', style: { top: (100 - i * 25) + '%' } }, fmt(t)); }),
        h('span', { className: 'fg-plot-x', style: { left: 0 } }, '−' + (p.windowLabel || (n * (p.interval || 50) / 1000).toFixed(0) + ' s')),
        h('span', { className: 'fg-plot-x', style: { right: 0 } }, 'jetzt'),
        h('svg', { viewBox: '0 0 ' + W + ' ' + H, preserveAspectRatio: 'none', 'aria-hidden': true },
          ticks.map(function (t, i) { return h('line', { key: i, className: cx('fg-plot-grid', t === 0 && 'fg-plot-zero'), x1: 0, x2: W, y1: y(t), y2: y(t) }); }),
          series.map(function (s, i) {
            var v = s.values.slice(-n), off = n - v.length;
            var d = v.map(function (val, j) { return (j ? 'L' : 'M') + ((j + off) / (n - 1) * W).toFixed(2) + ' ' + y(val).toFixed(2); }).join('');
            return h('path', { key: s.name, className: 'fg-plot-line fg-pl-' + (i + 1), d: d });
          })),
        h('span', { className: 'fg-plot-now' }),
        series.map(function (s, i) {
          var last = s.values[s.values.length - 1];
          return last == null ? null : h('span', { key: s.name, className: 'fg-plot-head-dot d-' + (i + 1), style: { top: (y(last)) + '%' } });
        })));
  }

  /* ---------- BuildProgress ---------- */
  var STAGES = ['Kompilieren', 'Linken', 'Verbinden', 'Löschen', 'Schreiben', 'Prüfen'];
  function BuildProgress(p) {
    var stages = p.stages || STAGES, st = p.status || 'running', cur = p.stage || 0, prog = p.progress || 0;
    var total = st === 'done' ? 1 : (cur + prog) / stages.length;
    var state = st === 'done' ? 'connected' : st === 'error' ? 'error' : (cur <= 1 ? 'compiling' : 'flashing');
    return h('div', { className: cx('fg-build', 'is-' + st, p.className), role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': 100, 'aria-valuenow': Math.round(total * 100), 'aria-label': p.title || 'Build' },
      h('div', { className: 'fg-build-top' },
        h(StatusDot, { state: state, label: false }),
        h('div', { className: 'fg-build-title' }, p.title || 'Flashe Sketch', p.target ? h('span', null, ' → ' + p.target) : null),
        h('span', { className: 'fg-build-pct' }, st === 'error' ? 'Abbruch' : Math.round(total * 100) + ' %')),
      h('div', { className: 'fg-build-track' }, stages.map(function (s, i) {
        var done = st === 'done' || i < cur, active = st === 'running' && i === cur, err = st === 'error' && i === cur;
        var w = done ? 100 : (active || err) ? prog * 100 : 0;
        return h('div', { key: s, className: cx('fg-stage', done && 'is-done', active && 'is-active', err && 'is-error') },
          h('div', { className: 'fg-stage-bar' }, h('span', { className: 'fg-stage-fill', style: { width: w + '%' } })),
          h('div', { className: 'fg-stage-label' }, done ? h(Icon, { name: 'check', size: 12 }) : err ? h(Icon, { name: 'error', size: 12 }) : null, s));
      })),
      p.detail ? h('div', { className: 'fg-build-detail' }, p.detail) : null);
  }

  /* ---------- Pinout (ESP32-S3-DevKitC-1) ---------- */
  var S3_LEFT = ['3V3', '3V3', 'RST', 4, 5, 6, 7, 15, 16, 17, 18, 8, 3, 46, 9, 10, 11, 12, 13, 14, '5V', 'GND'];
  var S3_RIGHT = ['GND', 43, 44, 1, 2, 42, 41, 40, 39, 38, 37, 36, 35, 0, 45, 48, 47, 21, 20, 19, 'GND', 'GND'];
  function pinCaps(g) {
    var c = [];
    if (g >= 1 && g <= 10) c.push('ADC1_CH' + (g - 1));
    if (g >= 11 && g <= 20) c.push('ADC2_CH' + (g - 11));
    if (g >= 1 && g <= 14) c.push('TOUCH' + g);
    if (g === 19) c.push('USB D−'); if (g === 20) c.push('USB D+');
    if (g === 43) c.push('U0TXD'); if (g === 44) c.push('U0RXD');
    if ([0, 3, 45, 46].indexOf(g) >= 0) c.push('Strapping-Pin');
    if ([35, 36, 37].indexOf(g) >= 0) c.push('Octal-PSRAM (nicht frei)');
    if (g === 48) c.push('RGB-LED');
    return c;
  }
  function Pinout(p) {
    var used = p.used || {};
    var hs = useState(p.highlight != null ? p.highlight : null);
    var hot = hs[0];
    function kind(x) { return x === 'GND' ? 'gnd' : (x === '3V3' || x === '5V') ? 'power' : x === 'RST' ? 'ctl' : 'gpio'; }
    function label(x) { return typeof x === 'number' ? (x === 43 ? 'TX 43' : x === 44 ? 'RX 44' : String(x)) : x; }
    function pin(x, side, row) {
      var k = kind(x), u = typeof x === 'number' && used[x], isHot = hot === x && typeof x === 'number';
      var cells = [
        h('span', { key: 'n', className: 'fg-pin-num' }, label(x)),
        u ? h('span', { key: 'f', className: 'fg-pin-fn' }, u) : null
      ];
      if (side === 'l') cells.reverse();
      var lab = h('div', {
        key: side + row + 'l', tabIndex: typeof x === 'number' ? 0 : -1, className: cx('fg-pin', 'is-' + k, side === 'l' && 'is-left', isHot && 'is-hot'),
        style: { gridRow: row + 1, gridColumn: side === 'l' ? 1 : 5 },
        onMouseEnter: function () { if (typeof x === 'number') hs[1](x); }, onFocus: function () { if (typeof x === 'number') hs[1](x); }
      }, cells);
      var pad = h('span', { key: side + row + 'p', className: cx('fg-pad', 'is-' + k, u && 'is-used', isHot && 'is-hot'), style: { gridRow: row + 1, gridColumn: side === 'l' ? 2 : 4 } });
      return [lab, pad];
    }
    var info = hot != null ? [h('strong', { key: 's' }, 'GPIO ' + hot)].concat(pinCaps(hot).map(function (c) { return h(Badge, { key: c, mono: true }, c); })).concat(used[hot] ? [h(Badge, { key: 'u', tone: 'glut', mono: true }, 'belegt: ' + used[hot])] : [h('span', { key: 'f' }, 'frei')]) : [h('span', { key: 'x' }, 'Pin wählen, um Funktionen zu sehen.')];
    var ledOn = p.ledOn;
    return h('div', { className: cx('fg-pinout', p.className) },
      h('div', { className: 'fg-pin-grid' },
        S3_LEFT.map(function (x, i) { return pin(x, 'l', i); }),
        h('div', { key: 'board', className: 'fg-board', 'aria-hidden': true },
          h('div', { className: 'fg-board-mod' }, h('b', null, 'ESP32-S3'), h('span', null, 'WROOM-1', h('br'), 'N16R8')),
          h('span', { className: 'fg-board-antenna' }),
          h('span', { className: cx('fg-board-led', ledOn && 'is-on') }),
          h('span', { className: 'fg-board-name' }, p.boardLabel || 'DevKitC-1'),
          h('span', { className: 'fg-board-usb', style: { left: 18 } }),
          h('span', { className: 'fg-board-usb', style: { right: 18 } })),
        S3_RIGHT.map(function (x, i) { return pin(x, 'r', i); })),
      h('div', { className: 'fg-pin-info', 'aria-live': 'polite' }, info),
      p.legend === false ? null : h('div', { className: 'fg-pin-legend' },
        h('span', null, h('i', { className: 'fg-pad is-used' }), 'im Sketch belegt'),
        h('span', null, h('i', { className: 'fg-pad' }), 'GPIO'),
        h('span', null, h('i', { className: 'fg-pad is-power' }), 'Versorgung'),
        h('span', null, h('i', { className: 'fg-pad is-gnd' }), 'GND')));
  }

  /* ---------- MemoryMeter, PartitionMap ---------- */
  function fmtBytes(b) { return b >= 1048576 ? (b / 1048576).toFixed(2) + ' MB' : b >= 1024 ? (b / 1024).toFixed(1) + ' KB' : b + ' B'; }
  function MemoryMeter(p) {
    var warn = p.warnAt || .8, crit = p.errorAt || .95;
    return h('div', { className: cx('fg-mem', p.large && 'is-large', p.className) }, (p.items || []).map(function (it) {
      var f = Math.min(1, it.used / it.total), lvl = f >= crit ? 'error' : f >= warn ? 'warning' : null;
      return h('div', { key: it.label, role: 'meter', 'aria-valuemin': 0, 'aria-valuemax': it.total, 'aria-valuenow': it.used, 'aria-label': it.label },
        h('div', { className: 'fg-mem-head' },
          h('span', { className: 'fg-mem-label' }, it.label),
          h('span', { className: 'fg-mem-val' }, h('b', null, fmtBytes(it.used)), ' / ' + fmtBytes(it.total)),
          h('span', { className: 'fg-mem-pct' }, Math.round(f * 100) + '%')),
        h('div', { className: 'fg-meter' }, h('span', { className: cx('fg-meter-fill', lvl && 'is-' + lvl), style: { width: f * 100 + '%' } })),
        lvl ? h('div', { className: 'fg-mem-note is-' + lvl }, h(Icon, { name: lvl === 'error' ? 'error' : 'warning', size: 12 }), it.note || (lvl === 'error' ? 'Kaum noch Platz — größeres Partitionsschema wählen.' : 'Wird knapp.')) : null);
    }));
  }
  function hex(n) { return '0x' + n.toString(16).toUpperCase().padStart(6, '0'); }
  function PartitionMap(p) {
    var parts = p.partitions || [], total = p.flashSize || parts.reduce(function (a, x) { return Math.max(a, x.offset + x.size); }, 0);
    var free = total - parts.reduce(function (a, x) { return Math.max(a, x.offset + x.size); }, 0);
    var rows = parts.concat(free > 0 ? [{ name: 'frei', type: '—', offset: total - free, size: free, free: true }] : []);
    return h('div', { className: cx('fg-parts', p.className) },
      h('div', { className: 'fg-parts-bar', role: 'img', 'aria-label': 'Partitionstabelle' }, rows.map(function (x, i) {
        return h('span', { key: x.name, className: 'fg-part c-' + (x.free ? 6 : (x.color || (i % 5) + 1)), style: { flex: x.size + ' 0 0' }, title: x.name + ' · ' + fmtBytes(x.size) },
          x.used ? h('span', { className: 'fg-part-used', style: { width: Math.min(100, x.used / x.size * 100) + '%' } }) : null);
      })),
      h('table', { className: 'fg-parts-table' }, h('tbody', null, rows.map(function (x, i) {
        return h('tr', { key: x.name },
          h('td', { className: 'n' }, h('i', { className: 'fg-sw c-' + (x.free ? 6 : (x.color || (i % 5) + 1)) }), x.name),
          h('td', null, x.type),
          h('td', { className: 'm' }, hex(x.offset)),
          h('td', { className: 'm' }, x.used ? fmtBytes(x.used) + ' / ' : '', fmtBytes(x.size)));
      }))));
  }

  /* ---------- LibraryItem ---------- */
  function LibraryItem(p) {
    var st = p.state || 'available';
    return h('div', { className: cx('fg-lib', p.className) },
      h('div', { className: 'fg-lib-name' }, h('span', null, p.name), h(Badge, { mono: true }, p.version),
        st === 'installed' ? h(Badge, { tone: 'success', icon: 'check' }, 'Installiert') : null,
        st === 'update' ? h(Badge, { tone: 'info', icon: 'arrow-down' }, p.latest ? p.latest + ' verfügbar' : 'Update') : null),
      h('div', { className: 'fg-lib-meta' }, p.author, p.downloads ? h('span', null, ' · ', h('span', { className: 'fg-mono' }, p.downloads), ' Installationen') : null),
      p.description ? h('div', { className: 'fg-lib-desc' }, p.description) : null,
      h('div', { className: 'fg-lib-act' },
        st === 'available' ? h(Button, { size: 'sm', icon: 'plus', onClick: p.onInstall }, 'Installieren') : null,
        st === 'installing' ? h(Button, { size: 'sm', busy: true, disabled: true }, 'Installiere …') : null,
        st === 'update' ? h(Button, { size: 'sm', variant: 'primary', icon: 'arrow-down', onClick: p.onUpdate }, 'Aktualisieren') : null,
        st === 'installed' ? h(Button, { size: 'sm', variant: 'ghost', icon: 'more', 'aria-label': 'Optionen für ' + p.name }) : null));
  }

  /* ---------- Traces (generative), EmptyState, Splash ---------- */
  function rng(seed) { var s = (seed >>> 0) || 1; return function () { s ^= s << 13; s ^= s >>> 17; s ^= s << 5; return ((s >>> 0) % 100000) / 100000; }; }
  var DIRS = [[1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0], [-1, -1], [0, -1], [1, -1]];
  function Traces(p) {
    var W = p.width || 640, H = p.height || 360, P = p.pitch || 24, count = p.count || 16, lit = p.lit == null ? 3 : p.lit;
    var data = useMemo(function () {
      var r = rng(p.seed || 7), cols = Math.floor(W / P), rows = Math.floor(H / P), out = [];
      for (var i = 0; i < count; i++) {
        var x = (1 + Math.floor(r() * (cols - 1))) * P, y = (1 + Math.floor(r() * (rows - 1))) * P, pts = [[x, y]];
        var dir = Math.floor(r() * 4) * 2, segs = 2 + Math.floor(r() * 3);
        for (var k = 0; k < segs; k++) {
          var d = DIRS[dir], len = (dir % 2 ? 1 + Math.floor(r() * 3) : 2 + Math.floor(r() * 6)) * P;
          x += d[0] * len; y += d[1] * len; pts.push([x, y]);
          dir = (dir + (r() < .5 ? 1 : 7)) % 8;
        }
        out.push(pts);
      }
      return out;
    }, [p.seed, W, H, P, count]);
    var dots = [];
    if (p.raster !== false) for (var gx = P; gx < W; gx += P) for (var gy = P; gy < H; gy += P) dots.push(h('circle', { key: gx + '-' + gy, className: 'dot', cx: gx, cy: gy, r: 1.1 }));
    return h('svg', { className: cx('fg-traces', p.className), viewBox: '0 0 ' + W + ' ' + H, preserveAspectRatio: 'xMidYMid slice', 'aria-hidden': true },
      dots,
      data.map(function (pts, i) {
        var d = pts.map(function (q) { return q.join(','); }).join(' ');
        var end = pts[pts.length - 1], start = pts[0];
        return h('g', { key: i },
          h('polyline', { className: 't', points: d }),
          i < lit && p.animate !== false ? h('polyline', { className: 'lit', points: d, pathLength: 100, style: { animationDelay: (i * 1.1) + 's' } }) : null,
          h('circle', { className: 'pad', cx: start[0], cy: start[1], r: 3.5 }),
          h('circle', { className: 'pad', cx: end[0], cy: end[1], r: 3.5 }));
      }));
  }

  function EmptyState(p) {
    return h('div', { className: cx('fg-empty', p.className), style: p.height ? { height: p.height, minHeight: 0 } : null },
      p.pattern === false ? null : h('div', { className: 'fg-empty-art' }, h(Traces, { seed: p.seed || 11, count: p.count || 22, lit: p.lit == null ? 2 : p.lit, width: 960, height: 540 })),
      h('div', { className: 'fg-empty-body' },
        p.icon ? h('span', { className: 'fg-empty-icon' }, h(Icon, { name: p.icon, size: 20 })) : null,
        h('h3', { className: 'fg-empty-title' }, p.title),
        p.text ? h('p', { className: 'fg-empty-text' }, p.text) : null,
        p.actions ? h('div', { className: 'fg-empty-actions' }, p.actions) : null,
        p.hints ? h('div', { className: 'fg-empty-hints' }, p.hints.reduce(function (acc, x, i) {
          acc.push(h(Kbd, { key: 'k' + i, keys: x.keys }), h('span', { key: 't' + i }, x.label)); return acc;
        }, [])) : null));
  }

  function Splash(p) {
    return h('div', { className: cx('fg-splash', p.className), role: 'img', 'aria-label': 'Forge wird gestartet' },
      h('div', { className: 'fg-splash-art' }, h(Traces, { seed: p.seed || 3, count: 14, lit: 3, width: 560, height: 340 })),
      h('div', { className: 'fg-splash-body' },
        h('div', { className: 'fg-splash-mark' }, 'forge', h('b')),
        h('div', { className: 'fg-splash-load' }, h('i', { style: { width: Math.round((p.progress == null ? .6 : p.progress) * 100) + '%' } })),
        h('div', { className: 'fg-splash-meta' }, h('span', null, p.status || 'Toolchain wird geladen · esp32 3.1.1'), h('span', null, p.version || 'v0.9.0'))));
  }

  var C = window;
  C.Forge = C.Forge || {};
  Object.assign(C.Forge, {
    Icon: Icon, Button: Button, Kbd: Kbd, Badge: Badge, StatusDot: StatusDot, Input: Input, Select: Select,
    Tabs: Tabs, Panel: Panel, Sidebar: Sidebar, ActivityBar: ActivityBar, FileTree: FileTree,
    StatusBar: StatusBar, StatusItem: StatusItem, TitleBar: TitleBar, CommandPalette: CommandPalette,
    Toast: Toast, ToastStack: ToastStack, Dialog: Dialog, CodeEditor: CodeEditor, SerialMonitor: SerialMonitor,
    Plotter: Plotter, BuildProgress: BuildProgress, Pinout: Pinout, MemoryMeter: MemoryMeter, PartitionMap: PartitionMap,
    LibraryItem: LibraryItem, Traces: Traces, EmptyState: EmptyState, Splash: Splash, Toggle: Toggle
  });
})();
