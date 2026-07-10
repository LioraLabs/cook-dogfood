using System.Text.Json;

public record MenuItem(string Name, double Price, string? Tags);

public static class MenuLoader
{
    public static List<MenuItem> Load(string path)
    {
        var opts = new JsonSerializerOptions { PropertyNameCaseInsensitive = true };
        var items = JsonSerializer.Deserialize<List<MenuItem>>(File.ReadAllText(path), opts)
            ?? throw new InvalidDataException("menu.json deserialised to null");
        if (items.Count == 0) throw new InvalidDataException("menu.json has no items");
        return items;
    }
}

public class Program
{
    public static void Main(string[] args)
    {
        var builder = WebApplication.CreateBuilder(args);
        var app = builder.Build();
        var menuPath = Environment.GetEnvironmentVariable("MENU_JSON") ?? "menu.json";
        app.MapGet("/menu", () => MenuLoader.Load(menuPath));
        app.MapGet("/menu/{name}", (string name) =>
            MenuLoader.Load(menuPath).FirstOrDefault(i => i.Name == name) is { } item
                ? Results.Ok(item)
                : Results.NotFound());
        app.Run();
    }
}
