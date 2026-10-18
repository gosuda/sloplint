using System;
using System.IO;

public class Loader {
    public string Load(string path) {
        try {
            return Read(path);
        } catch (IOException e) {
            throw new AppException("load failed", e);
        }
    }

    private string Read(string path) {
        return path;
    }
}

public class AppException : Exception {
    public AppException(string message, Exception inner) : base(message, inner) {
    }
}
