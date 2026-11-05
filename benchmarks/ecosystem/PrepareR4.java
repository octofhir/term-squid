import java.nio.file.Files;
import java.nio.file.Path;
import org.hl7.fhir.convertors.factory.VersionConvertorFactory_40_50;

/** Convert official R5 setup resources with the same converter family as the official runner. */
class PrepareR4 {
  public static void main(String[] args) throws Exception {
    for (String line : Files.readAllLines(Path.of(args[0]))) {
      String[] paths = line.split("\t", -1);
      if (paths.length != 2) throw new IllegalArgumentException("Expected source and output paths");
      var source = new org.hl7.fhir.r5.formats.JsonParser().parse(Files.readAllBytes(Path.of(paths[0])));
      var resource = VersionConvertorFactory_40_50.convertResource(source);
      var bytes = new org.hl7.fhir.r4.formats.JsonParser().composeBytes(resource);
      Files.write(Path.of(paths[1]), bytes);
    }
  }
}
