import java.io.ByteArrayOutputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import org.tukaani.xz.LZMA2Options;
import org.tukaani.xz.XZ;
import org.tukaani.xz.XZOutputStream;

public class DhsFixtures {
    public static void main(String[] args) throws Exception {
        for (int i = 0; i + 1 < args.length; i += 2) {
            byte[] in = Files.readAllBytes(Path.of(args[i]));
            LZMA2Options options = new LZMA2Options(3);
            int dict = LZMA2Options.DICT_SIZE_MIN;
            while (dict < in.length && dict < 1 << 24) dict <<= 1;
            options.setDictSize(dict);
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            OutputStream xz = new XZOutputStream(out, options, XZ.CHECK_CRC64);
            xz.write(in);
            xz.flush();
            Files.write(Path.of(args[i + 1]), out.toByteArray());
        }
    }
}
