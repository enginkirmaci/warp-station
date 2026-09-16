using CommunityToolkit.Mvvm.ComponentModel;
using OpenCodeAgent.Models;

namespace OpenCodeAgent.ViewModels;

public sealed class SessionItem : ObservableObject
{
    private string _title;
    private DateTime _updated;
    private bool _isActive;
    private bool _isEditing;
    private string _draftTitle = "";

    public SessionItem(string id, string title, DateTime updated)
    {
        Id = id;
        _title = title;
        _updated = updated;
    }

    public string Id { get; }

    public string Title { get => _title; set => SetProperty(ref _title, value); }

    public DateTime Updated
    {
        get => _updated;
        set
        {
            if (SetProperty(ref _updated, value))
                OnPropertyChanged(nameof(UpdatedLabel));
        }
    }

    public bool IsActive { get => _isActive; set => SetProperty(ref _isActive, value); }

    public bool IsEditing { get => _isEditing; set => SetProperty(ref _isEditing, value); }

    public string DraftTitle { get => _draftTitle; set => SetProperty(ref _draftTitle, value); }

    public string UpdatedLabel => TimeLabels.Relative(Updated);
}
