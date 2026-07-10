using Xunit;

public class MenuTests
{
    private static string MenuPath =>
        Environment.GetEnvironmentVariable("MENU_JSON")
        ?? throw new InvalidOperationException("MENU_JSON not set (cook injects it)");

    [Fact]
    public void Menu_loads_and_is_nonempty()
    {
        Assert.NotEmpty(MenuLoader.Load(MenuPath));
    }

    [Fact]
    public void Menu_items_have_positive_prices_and_unique_names()
    {
        var items = MenuLoader.Load(MenuPath);
        Assert.All(items, i => Assert.True(i.Price > 0, $"{i.Name} has non-positive price"));
        Assert.Equal(items.Count, items.Select(i => i.Name).Distinct().Count());
    }
}
