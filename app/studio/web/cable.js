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
  window.composerCable = {
    name: NAME,
    connected: () => typeof input.onmidimessage === 'function',
    // fm1-emu's load (time spent emulating per time emulated, from its status line: "load 63%"); above 1 it runs
    // slower than real time, and its MIDI IN jack takes bytes that much slower. 1 while it is not shown yet
    load() {
      const m = /· load (\d+)%/.exec(document.querySelector('#status')?.textContent || '');
      return m ? Math.max(0.01, Number(m[1]) / 100) : 1;
    },
    send(bytes) {
      if (typeof input.onmidimessage !== 'function') throw new Error(`choose "${NAME}" in the FM-1's MIDI in`);
      input.onmidimessage({ data: Uint8Array.from(bytes), timeStamp: performance.now() });
    },
  };
})();
