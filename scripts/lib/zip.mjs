/**
 * A small ZIP writer and reader (deflate or stored), enough for the
 * connector bundle (`.mcpb` is a ZIP archive, MCPB MANIFEST.md). Times are
 * fixed (1980-01-01) so the same files always give the same bytes.
 */
import { crc32, deflateRawSync, inflateRawSync } from 'node:zlib';

const DOS_TIME = 0;
const DOS_DATE = (0 << 9) | (1 << 5) | 1; // 1980-01-01

/** @param {{ name: string, data: Uint8Array }[]} files */
export function zip(files) {
	const locals = [];
	const centrals = [];
	let offset = 0;
	for (const f of files) {
		const name = Buffer.from(f.name, 'utf8');
		const raw = Buffer.from(f.data);
		const packed = deflateRawSync(raw, { level: 9 });
		const useDeflate = packed.length < raw.length;
		const body = useDeflate ? packed : raw;
		const crc = crc32(raw) >>> 0;
		const local = Buffer.alloc(30);
		local.writeUInt32LE(0x04034b50, 0);
		local.writeUInt16LE(20, 4);
		local.writeUInt16LE(0x0800, 6); // UTF-8 names
		local.writeUInt16LE(useDeflate ? 8 : 0, 8);
		local.writeUInt16LE(DOS_TIME, 10);
		local.writeUInt16LE(DOS_DATE, 12);
		local.writeUInt32LE(crc, 14);
		local.writeUInt32LE(body.length, 18);
		local.writeUInt32LE(raw.length, 22);
		local.writeUInt16LE(name.length, 26);
		local.writeUInt16LE(0, 28);
		locals.push(local, name, body);
		const central = Buffer.alloc(46);
		central.writeUInt32LE(0x02014b50, 0);
		central.writeUInt16LE(20, 4);
		central.writeUInt16LE(20, 6);
		central.writeUInt16LE(0x0800, 8);
		central.writeUInt16LE(useDeflate ? 8 : 0, 10);
		central.writeUInt16LE(DOS_TIME, 12);
		central.writeUInt16LE(DOS_DATE, 14);
		central.writeUInt32LE(crc, 16);
		central.writeUInt32LE(body.length, 20);
		central.writeUInt32LE(raw.length, 24);
		central.writeUInt16LE(name.length, 28);
		central.writeUInt32LE(offset, 42);
		centrals.push(central, name);
		offset += local.length + name.length + body.length;
	}
	const cd = Buffer.concat(centrals);
	const end = Buffer.alloc(22);
	end.writeUInt32LE(0x06054b50, 0);
	end.writeUInt16LE(files.length, 8);
	end.writeUInt16LE(files.length, 10);
	end.writeUInt32LE(cd.length, 12);
	end.writeUInt32LE(offset, 16);
	return Buffer.concat([...locals, cd, end]);
}

/** Reads a ZIP written by `zip` (or any plain one): name to bytes. */
export function unzip(buf) {
	const b = Buffer.from(buf);
	const endAt = b.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
	if (endAt < 0) throw new Error('not a zip');
	const count = b.readUInt16LE(endAt + 10);
	let p = b.readUInt32LE(endAt + 16);
	const out = new Map();
	for (let i = 0; i < count; i++) {
		if (b.readUInt32LE(p) !== 0x02014b50) throw new Error('bad central directory');
		const method = b.readUInt16LE(p + 10);
		const crc = b.readUInt32LE(p + 16);
		const size = b.readUInt32LE(p + 20);
		const nameLen = b.readUInt16LE(p + 28);
		const extraLen = b.readUInt16LE(p + 30);
		const commentLen = b.readUInt16LE(p + 32);
		const local = b.readUInt32LE(p + 42);
		const name = b.subarray(p + 46, p + 46 + nameLen).toString('utf8');
		const lNameLen = b.readUInt16LE(local + 26);
		const lExtraLen = b.readUInt16LE(local + 28);
		const start = local + 30 + lNameLen + lExtraLen;
		const body = b.subarray(start, start + size);
		const data = method === 8 ? inflateRawSync(body) : Buffer.from(body);
		if (crc32(data) >>> 0 !== crc) throw new Error(`bad checksum for ${name}`);
		out.set(name, data);
		p += 46 + nameLen + extraLen + commentLen;
	}
	return out;
}
