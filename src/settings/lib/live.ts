/**
 * The live preview's input: the renderer's TauriTransport, so "Live input"
 * shows exactly the `keystage://overlay` messages Host A receives (in the
 * app), or the mock backend's simulated input (in a browser).
 *
 * In the app the transport uses the renderer's own `loadTauriApi`, which
 * listens on this window only (target `WebviewWindow { label: 'settings' }`).
 * The shell's emit filter then decides whether this window gets overlay
 * messages: only while the preview can be seen, never while it is minimized.
 * A global (`Any`) listener would receive every message regardless.
 */
import { TauriTransport, type Transport } from '../../lib/renderer/transport';
import type { CommandName, KeystageIpc } from '../ipc';

export function liveTransport(ipc: KeystageIpc): Transport {
	if (ipc.backend.kind === 'tauri') return new TauriTransport();
	return new TauriTransport({
		available: () => true,
		loadApi: async () => ({
			listen: (event, handler) => ipc.backend.listen(event, (payload) => handler({ payload })),
			invoke: (cmd, args) => ipc.backend.invoke(cmd as CommandName, args),
		}),
	});
}
