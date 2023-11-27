using System.Collections;
using System.Text;
using Tools.Library.Extensions;

namespace Tools.Library.Entities;

public class Parameters : IEnumerable<KeyValuePair<string, object>>
{
    private readonly List<KeyValuePair<string, object>> _entries = new List<KeyValuePair<string, object>>();

    public object this[string key]
    {
        get
        {
            foreach (var entry in _entries)
            {
                if (string.Compare(entry.Key, key, StringComparison.Ordinal) == 0)
                {
                    return entry.Value;
                }
            }

            return null;
        }
    }

    public int Count => _entries.Count;

    public IEnumerable<string> Keys =>
        _entries.Select(x => x.Key);

    public Parameters()
    { }

    public Parameters(KeyValuePair<string, object>[] entries)
    {
        _entries = new List<KeyValuePair<string, object>>(entries);
    }

    public void Add(string key, object value) =>
        _entries.Add(new KeyValuePair<string, object>(key, value));

    public void Clear()
    {
        _entries.Clear();
    }

    public bool ContainsKey(string key) =>
        _entries.ContainsKey(key);

    public IEnumerator<KeyValuePair<string, object>> GetEnumerator() =>
        _entries.GetEnumerator();

    public T GetValue<T>(string key) =>
        _entries.GetValue<T>(key);

    public IEnumerable<T> GetValues<T>(string key) =>
        _entries.GetValues<T>(key);

    public bool TryGetValue<T>(string key, out T value) =>
      _entries.TryGetValue(key, out value);

    IEnumerator IEnumerable.GetEnumerator() =>
        GetEnumerator();

    public override string ToString()
    {
        var queryBuilder = new StringBuilder();

        if (_entries.Count > 0)
        {
            queryBuilder.Append('?');
            var first = true;

            foreach (var kvp in _entries)
            {
                if (!first)
                {
                    queryBuilder.Append('&');
                }
                else
                {
                    first = false;
                }

                queryBuilder.Append(Uri.EscapeDataString(kvp.Key));
                queryBuilder.Append('=');
                queryBuilder.Append(Uri.EscapeDataString(kvp.Value != null ? kvp.Value.ToString() : ""));
            }
        }

        return queryBuilder.ToString();
    }
}