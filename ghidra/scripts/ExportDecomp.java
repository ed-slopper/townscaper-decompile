// Export the pseudo-C of every game method listed in docs/function-index.csv to
// work/decomp/<Namespace>/<Type>/<Method>_<rva>.c, for agents without the Ghidra MCP.
//   args: <function-index.csv> <output dir>
// The output is decompiler output of the player's own copy: it stays in work/ (AGENTS.md section 13).
//@category Townscaper

import java.io.File;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileOptions;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;

public class ExportDecomp extends GhidraScript {

	/** Split one CSV line (fields may be quoted, quotes doubled inside). */
	private static List<String> csv(String line) {
		List<String> out = new ArrayList<>();
		StringBuilder cur = new StringBuilder();
		boolean q = false;
		for (int i = 0; i < line.length(); i++) {
			char c = line.charAt(i);
			if (q) {
				if (c == '"' && i + 1 < line.length() && line.charAt(i + 1) == '"') {
					cur.append('"');
					i++;
				}
				else if (c == '"') {
					q = false;
				}
				else {
					cur.append(c);
				}
			}
			else if (c == '"') {
				q = true;
			}
			else if (c == ',') {
				out.add(cur.toString());
				cur.setLength(0);
			}
			else {
				cur.append(c);
			}
		}
		out.add(cur.toString());
		return out;
	}

	private static String safe(String s) {
		String t = s.replaceAll("[<>:\"/\\\\|?*`]", "_").replace(' ', '_');
		return t.isEmpty() ? "_" : t;
	}

	@Override
	protected void run() throws Exception {
		String[] args = getScriptArgs();
		File index = new File(args[0]);
		File outDir = new File(args[1]);
		List<String> lines = Files.readAllLines(index.toPath(), StandardCharsets.UTF_8);
		List<String> header = csv(lines.get(0));
		int iRva = header.indexOf("rva"), iNs = header.indexOf("namespace"), iType = header.indexOf("type"),
			iMethod = header.indexOf("method"), iSig = header.indexOf("signature");

		DecompInterface di = new DecompInterface();
		di.setOptions(new DecompileOptions());
		di.openProgram(currentProgram);
		Address base = currentProgram.getImageBase();
		Set<Long> done = new HashSet<>();
		int ok = 0, failed = 0, shared = 0;
		monitor.initialize(lines.size() - 1);
		for (String line : lines.subList(1, lines.size())) {
			monitor.checkCancelled();
			monitor.incrementProgress(1);
			List<String> r = csv(line);
			long rva = Long.decode(r.get(iRva));
			String ns = r.get(iNs).isEmpty() ? "_global" : r.get(iNs);
			File dir = new File(new File(outDir, safe(ns)), safe(r.get(iType)));
			dir.mkdirs();
			File file = new File(dir, safe(r.get(iMethod)) + "_" + String.format("0x%X", rva) + ".c");
			if (!done.add(rva)) {
				shared++; // identical-code folding: one body, several C# methods; write a pointer
			}
			Function f = getFunctionContaining(base.add(rva));
			try (PrintWriter w = new PrintWriter(file, StandardCharsets.UTF_8)) {
				w.println("// " + ns + "." + r.get(iType) + "." + r.get(iMethod) + "  RVA " + String.format("0x%X", rva));
				w.println("// C#: " + r.get(iSig));
				if (f == null) {
					w.println("// no function at this address");
					failed++;
					continue;
				}
				if (f.getComment() != null) {
					w.println("/* " + f.getComment().replace("*/", "* /") + " */");
				}
				DecompileResults res = di.decompileFunction(f, 180, monitor);
				if (res.decompileCompleted()) {
					w.print(res.getDecompiledFunction().getC());
					ok++;
				}
				else {
					w.println("// decompile failed: " + res.getErrorMessage());
					failed++;
				}
			}
		}
		di.dispose();
		println(String.format("ExportDecomp: %d written, %d failed, %d rows share a body with an earlier row", ok, failed, shared));
	}
}
