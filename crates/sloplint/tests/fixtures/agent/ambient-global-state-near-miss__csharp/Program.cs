using System;

public static class Program {
    private static int counter;

    public static void Main() {
        counter = int.Parse(Environment.GetEnvironmentVariable("RETRIES") ?? "0");
    }
}
