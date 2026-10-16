// Apply Il2CppDumper's script.json to GameAssembly.dll: C# method names on functions, labels on
// metadata globals and string literals. Run after auto-analysis with the PDB loaded.
//   args: <path to work/il2cppdumper/script.json>
// PDB names (Placemaker_WaveFunctionCollapse__IterateDecor_m1234...) are kept; the C# name
// (Placemaker.WaveFunctionCollapse$$IterateDecor) goes into the plate comment. Functions that are
// still FUN_* get the C# name.
//@category Townscaper

import java.io.FileReader;
import java.util.HashMap;
import java.util.Map;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.SourceType;
import ghidra.program.model.symbol.SymbolTable;

public class ApplyIl2CppNames extends GhidraScript {

	private static String clean(String s) {
		return s.replaceAll("[^A-Za-z0-9_.$<>`]", "_");
	}

	@Override
	protected void run() throws Exception {
		String[] args = getScriptArgs();
		String path = args.length > 0 ? args[0] : askFile("script.json", "Apply").getAbsolutePath();
		JsonObject root;
		try (FileReader r = new FileReader(path)) {
			root = JsonParser.parseReader(r).getAsJsonObject();
		}
		Address base = currentProgram.getImageBase();
		FunctionManager fm = currentProgram.getFunctionManager();
		SymbolTable st = currentProgram.getSymbolTable();

		// Several C# methods can share one body (generic sharing, identical-code folding).
		Map<Long, StringBuilder> names = new HashMap<>();
		Map<Long, String> sigs = new HashMap<>();
		for (JsonElement e : root.getAsJsonArray("ScriptMethod")) {
			JsonObject m = e.getAsJsonObject();
			long rva = m.get("Address").getAsLong();
			names.computeIfAbsent(rva, k -> new StringBuilder())
				.append(names.get(rva).length() > 0 ? "\n" : "")
				.append(m.get("Name").getAsString());
			if (m.has("Signature") && !m.get("Signature").isJsonNull()) {
				sigs.putIfAbsent(rva, m.get("Signature").getAsString());
			}
		}
		int created = 0, renamed = 0, commented = 0;
		monitor.initialize(names.size());
		monitor.setMessage("methods");
		for (Map.Entry<Long, StringBuilder> en : names.entrySet()) {
			monitor.checkCancelled();
			monitor.incrementProgress(1);
			Address a = base.add(en.getKey());
			Function f = fm.getFunctionAt(a);
			if (f == null) {
				disassemble(a);
				f = createFunction(a, null);
				if (f == null) {
					continue;
				}
				created++;
			}
			String all = en.getValue().toString();
			String first = all.split("\n")[0];
			if (f.getName().startsWith("FUN_")) {
				f.setName(clean(first), SourceType.ANALYSIS);
				renamed++;
			}
			String plate = "C#: " + all.replace("\n", "\nC#: ");
			if (sigs.containsKey(en.getKey())) {
				plate += "\nil2cpp: " + sigs.get(en.getKey());
			}
			f.setComment(plate);
			commented++;
		}

		int labels = 0;
		for (String key : new String[] { "ScriptMetadata", "ScriptMetadataMethod" }) {
			JsonArray arr = root.getAsJsonArray(key);
			monitor.setMessage(key);
			for (JsonElement e : arr) {
				monitor.checkCancelled();
				JsonObject m = e.getAsJsonObject();
				Address a = base.add(m.get("Address").getAsLong());
				st.createLabel(a, clean(m.get("Name").getAsString()), SourceType.ANALYSIS);
				labels++;
			}
		}
		int strings = 0;
		for (JsonElement e : root.getAsJsonArray("ScriptString")) {
			monitor.checkCancelled();
			JsonObject m = e.getAsJsonObject();
			Address a = base.add(m.get("Address").getAsLong());
			String v = m.get("Value").getAsString();
			String label = "StringLiteral_" + clean(v.length() > 40 ? v.substring(0, 40) : v);
			st.createLabel(a, label, SourceType.ANALYSIS);
			setEOLComment(a, v.length() > 200 ? v.substring(0, 200) + "..." : v);
			strings++;
		}
		println(String.format("ApplyIl2CppNames: %d functions commented, %d created, %d renamed, %d labels, %d strings",
			commented, created, renamed, labels, strings));
	}
}
