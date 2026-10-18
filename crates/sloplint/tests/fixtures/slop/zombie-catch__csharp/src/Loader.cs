using System;
using System.IO;

public class Loader {
    public string Load(string path) {
        try {
            return Read(path);
        } catch (IOException) {
            throw;
        }
    }

    private string Read(string path) {
        return path;
    }
}
