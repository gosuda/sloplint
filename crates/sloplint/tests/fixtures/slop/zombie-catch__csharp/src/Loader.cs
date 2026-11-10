using System;
using System.IO;

public class Loader {
    public string Load(string path) {
        try {
            return Read(path);
        } catch (IOException e) {
            logger.LogError("load failed");
            throw;
        }
    }

    private string Read(string path) {
        return path;
    }
}
