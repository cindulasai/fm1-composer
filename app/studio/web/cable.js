// COMPOSER Studio's MIDI cable, loaded into fm1-emu's own page (fm1c studio serves that page with this one script
// added before its own). It gives the page a MIDI input named "COMPOSER Studio", as a USB MIDI keyboard would, through
// the browser's Web MIDI interface that fm1-emu already uses; fm1-emu's code is not changed. Studio's Send button
// plays a piece's transfer SysEx into it, and fm1-emu passes each message to the FM-1's MIDI IN jack.
(() => {
  const NAME = 'COMPOSER Studio';
  const input = { id: 'composer-studio', name: NAME, manufacturer: 'COMPOSER', type: 'input', state: 'connected',
                  connection: 'open', onmidimessage: null, onstatechange: null,
                  addEventListener() {}, removeEventListener() {}, open: async () => input, close: async () => input };
  const real = navigator.requestMIDIAccess?.bind(navigator);
  const access = async (opts) => {
    let inputs = new Map(), outputs = new Map();
    if (real) { try { const a = await real(opts); inputs = new Map(a.inputs); outputs = a.outputs; } catch { /* none */ } }
    inputs.set(input.id, input);
    return { inputs, outputs, sysexEnabled: true, onstatechange: null, addEventListener() {}, removeEventListener() {} };
  };
  Object.defineProperty(navigator, 'requestMIDIAccess', { value: access, configurable: true });
  try {                                                       // first visit: Studio's cable is the page's MIDI in
    if (!localStorage.getItem('fm1.midi')) localStorage.setItem('fm1.midi', NAME);
  } catch { /* private mode: pick it in MIDI in */ }
  // the FM-1's screen as fm1-emu's audio thread sends it to its page (a "frame" message with the LCD, RGB565 240 x 240):
  // Studio reads the footer off it (US-34: NEXT: / LIVE: <name>); the message goes on to fm1-emu unchanged
  let screen = null;
  const watchers = new Set();                 // Send's readers: every new screen, not only the latest (US-34.9)
  const onmsg = Object.getOwnPropertyDescriptor(MessagePort.prototype, 'onmessage');
  if (onmsg?.set) {
    Object.defineProperty(MessagePort.prototype, 'onmessage', {
      configurable: true, enumerable: onmsg.enumerable, get() { return onmsg.get.call(this); },
      set(fn) {
        onmsg.set.call(this, typeof fn !== 'function' ? fn : function (e) {
          const d = e.data;
          if (d && d.type === 'frame' && d.lcd instanceof Uint16Array && d.lcd.length === 240 * 240) {
            screen = d.lcd;
            for (const w of watchers) { try { w(screen); } catch { /* a reader's fault stays its own */ } }
          }
          return fn.call(this, e);
        });
      },
    });
  }
  window.composerCable = {
    name: NAME,
    screen: () => screen,                     // the latest screen (null before the first)
    onScreen(fn) { watchers.add(fn); return () => watchers.delete(fn); },   // fn(lcd) on each new screen; returns: stop
    connected: () => typeof input.onmidimessage === 'function',
    // fm1-emu's load (time spent emulating per time emulated, from its status line: "load 63%"); above 1 it runs
    // slower than real time, and its MIDI IN jack takes bytes that much slower. 1 while it is not shown yet
    load() {
      const m = /· load (\d+)%/.exec(document.querySelector('#status')?.textContent || '');
      return m ? Math.max(0.01, Number(m[1]) / 100) : 1;
    },
    // fm1-emu's emulated time in ms (its status line's last figure, "· 16.8 s"); NaN while it is not shown
    emulatedMs() {
      const m = /· (\d+(?:\.\d+)?) s$/.exec((document.querySelector('#status')?.textContent || '').trim());
      return m ? Number(m[1]) * 1000 : NaN;
    },
    send(bytes) {
      if (typeof input.onmidimessage !== 'function') throw new Error(`choose "${NAME}" in the FM-1's MIDI in`);
      input.onmidimessage({ data: Uint8Array.from(bytes), timeStamp: performance.now() });
    },
  };
  // fm1-emu's page on its own (not inside Studio): COMPOSER's tile explains itself and points to Studio, where pieces are
  // made (fm1-emu draws a tile from plain catalog text; this adds to that one tile only, its files stay unchanged)
  const STUDIO = new URL('../', location.href).href;
  const GUIDE = [
    ['p', ['Its library starts empty here. ', ['b', 'Start in three steps:']]],
    ['ol', [
      ['Open ', ['a', 'COMPOSER Studio ↗', STUDIO], ': describe the music ("slow lo-fi in D minor, 70 bpm") or pick an example (no AI key needed for those; your own key stays in your browser).'],
      ['Press ', ['b', 'Send to FM-1'], ': the piece lands in this FM-1’s LIBRARY.'],
      ['Press ', ['b', 'PLAY'], ' (or Space). KNOB1 picks a part, PRESETS changes its sound, white keys mute.'],
    ]],
    ['p', ['Then: ', ['b', 'ARP'], ' plays the keys in the piece’s key with auto-chords · ', ['b', 'EDIT'], ' piano roll · ', ['b', 'LFO'],
      ' step sequencer · ', ['b', 'REC'], ' records into a part · ', ['b', 'FX'], ' effects · ', ['a', 'full guide ↗', 'https://github.com/cindulasai/fm1-composer/blob/main/app/README.md#playing'], '.']],
  ];
  // fm1-emu styles its tiles' li / b / a; the guide's own elements start from the browser's defaults (all: revert)
  const node = (spec) => {
    if (typeof spec === 'string') return document.createTextNode(spec);
    const [tag, body, href] = spec;
    const e = document.createElement(tag === 'ol' ? 'div' : tag);
    e.style.cssText = 'all:revert';
    if (tag === 'a') { e.href = href; e.target = '_blank'; e.rel = 'noopener'; e.style.cssText = 'all:revert;color:#f5a623'; }
    if (tag === 'p') e.style.cssText = 'all:revert;margin:.3em 0';
    if (tag === 'ol') body.forEach((li, n) => {
      const l = document.createElement('div');
      l.style.cssText = 'all:revert;display:block;margin:.25em 0';
      l.append(`${n + 1}. `, ...li.map(node));
      e.append(l);
    });
    else if (Array.isArray(body)) e.append(...body.map(node));
    else e.textContent = body;
    return e;
  };
  const addGuide = () => {
    if (document.getElementById('composer-guide')) return true;
    const card = [...document.querySelectorAll('li')].find((li) => li.querySelector('.head b')?.textContent === NAME.split(' ')[0]);
    if (!card) return false;
    const box = document.createElement('div');
    box.id = 'composer-guide';
    box.style.cssText = 'margin:.5em 0;padding:.6em .8em;border-left:3px solid #f5a623;background:rgba(245,166,35,.08);font-size:.92em;line-height:1.45';
    // the way in, first and big: Studio is where a piece is made (this page alone has nothing to play yet)
    const cta = document.createElement('a');
    cta.className = 'composer-cta'; cta.href = STUDIO; cta.target = '_blank'; cta.rel = 'noopener';
    cta.textContent = '\u{1F3B9} Compose music with AI: open COMPOSER Studio \u2192';
    cta.style.cssText = 'all:revert;display:block;margin:.2em 0 .6em;padding:.7em 1em;border-radius:10px;background:#f5a623;' +
      'color:#1a1a1a;font-weight:700;text-align:center;text-decoration:none;font-size:1.05em';
    const sub = document.createElement('div');
    sub.style.cssText = 'all:revert;display:block;margin:0 0 .5em;font-size:.9em;opacity:.85';
    sub.textContent = 'Type "slow lo-fi in D minor" \u2192 an AI writes drums, bass, chords and lead \u2192 one click plays it on this FM-1.';
    box.append(cta, sub, ...GUIDE.map(node));
    (card.querySelector('p.what') || card.lastChild).after(box);
    return true;
  };
  if (new URLSearchParams(location.search).get('embed') !== '1') {
    const watch = new MutationObserver(() => { if (addGuide()) watch.disconnect(); });
    const go = () => { if (!addGuide()) watch.observe(document.body, { childList: true, subtree: true }); };
    if (document.body) go(); else document.addEventListener('DOMContentLoaded', go);
  }
})();
