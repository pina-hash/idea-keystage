/**
 * The live preview's input: the renderer's TauriTransport on top of this
 * window's IPC backend, so "Live input" shows exactly the `keystage://overlay`
 * messages Host A receives (in the app), or the mock backend's simulated
 * input (in a browser).
 */
import { TauriTransport, type Transport } from '../../lib/renderer/transport';
import type { CommandName, KeystageIpc } from '../ipc';

export function liveTransport(ipc: KeystageIpc): Transport {
	return new TauriTransport({
		available: () => true,
		loadApi: async () => ({
			listen: (event, handler) => ipc.backend.listen(event, (payload) => handler({ payload })),
			invoke: (cmd, args) => ipc.backend.invoke(cmd as CommandName, args),
		}),
	});
}
