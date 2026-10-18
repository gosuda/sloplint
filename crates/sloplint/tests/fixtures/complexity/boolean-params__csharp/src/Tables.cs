public class Tables {
    public string RenderTable(bool header, bool footer) {
        string result = "";
        if (header) { result += "h"; }
        if (footer) { result += "f"; }
        return result;
    }
}
