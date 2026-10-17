// Print the instructions of the functions containing each given address (hex).
//   args: 0x1008ed50 [0x...]
//@category Townscaper

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;

public class DisassembleAt extends GhidraScript {
	@Override
	protected void run() throws Exception {
		for (String a : getScriptArgs()) {
			Address at = toAddr(Long.decode(a));
			Function f = getFunctionContaining(at);
			if (f == null) {
				println("==== " + a + "  (no function)");
				continue;
			}
			println("==== " + f.getEntryPoint() + "  " + f.getName(true));
			for (Instruction i : currentProgram.getListing().getInstructions(f.getBody(), true)) {
				println(i.getAddress() + "  " + i);
			}
		}
	}
}
