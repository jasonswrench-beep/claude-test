import java.io.FileInputStream;
import java.util.*;

import org.verapdf.gf.foundry.VeraGreenfieldFoundryProvider;
import org.verapdf.pdfa.Foundries;
import org.verapdf.pdfa.PDFAParser;
import org.verapdf.pdfa.PDFAValidator;
import org.verapdf.pdfa.flavours.PDFAFlavour;
import org.verapdf.pdfa.results.TestAssertion;
import org.verapdf.pdfa.results.ValidationResult;

/** Minimal PDF/UA-1 check using the veraPDF validation core. Usage: VerapdfCheck file.pdf... */
public class VerapdfCheck {
    public static void main(String[] args) throws Exception {
        VeraGreenfieldFoundryProvider.initialise();
        PDFAFlavour flavour = PDFAFlavour.PDFUA_1;
        for (String path : args) {
            System.out.println("== " + path);
            try (FileInputStream in = new FileInputStream(path);
                 PDFAParser parser = Foundries.defaultInstance().createParser(in, flavour)) {
                PDFAValidator validator = Foundries.defaultInstance().createValidator(flavour, false);
                ValidationResult result = validator.validate(parser);
                Map<String, Integer> counts = new TreeMap<>();
                Map<String, String> msgs = new HashMap<>();
                for (TestAssertion a : result.getTestAssertions()) {
                    if (a.getStatus() != TestAssertion.Status.FAILED) continue;
                    String id = a.getRuleId().getClause() + "-" + a.getRuleId().getTestNumber();
                    counts.merge(id, 1, Integer::sum);
                    msgs.putIfAbsent(id, a.getMessage());
                }
                int failedAssertions = counts.values().stream().mapToInt(Integer::intValue).sum();
                System.out.println("compliant=" + result.isCompliant()
                        + " failedRules=" + counts.size() + " failedAssertions=" + failedAssertions);
                for (Map.Entry<String, Integer> e : counts.entrySet()) {
                    String m = msgs.get(e.getKey()).replaceAll("\\s+", " ");
                    if (m.length() > 110) m = m.substring(0, 110) + "...";
                    System.out.println("  " + e.getKey() + " x" + e.getValue() + "  " + m);
                }
            } catch (Throwable t) {
                System.out.println("ERROR: " + t);
            }
        }
    }
}
