use gettextrs::gettext;
use gtk::prelude::*;
use std::rc::Rc;

use super::create_playlist::CreatePlaylistPopover;
use super::{
    sidebar_row::SidebarRow, SidebarDestination, SidebarItem, CREATE_PLAYLIST_ITEM,
    SAVED_PLAYLISTS_SECTION,
};
use crate::app::models::{AlbumModel, PlaylistSummary};
use crate::app::{
    ActionDispatcher, AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Component,
    EventListener,
};

const NUM_FIXED_ENTRIES: u32 = 6;
const NUM_PLAYLISTS: usize = 20;

pub struct SidebarModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
}

impl SidebarModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    fn get_playlists(&self) -> Vec<SidebarDestination> {
        self.app_model
            .get_state()
            .browser
            .home_state()
            .expect("expected HomeState to be available")
            .playlists
            .iter()
            .take(NUM_PLAYLISTS)
            .map(Self::map_to_destination)
            .collect()
    }

    fn map_to_destination(a: AlbumModel) -> SidebarDestination {
        let title = Some(a.album_title())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| gettext("Unnamed playlist"));
        let id = a.uri();
        SidebarDestination::Playlist(PlaylistSummary { id, title })
    }

    fn create_new_playlist(&self, name: String) {
        let user_id = self.app_model.get_state().logged_user.user.clone().unwrap();
        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch_many(move || async move {
                api.create_new_playlist(name.as_str(), user_id.as_str())
                    .await
                    .map(|p| {
                        vec![
                            BrowserAction::PrependPlaylistsContent(vec![p.clone()]).into(),
                            AppAction::ShowPlaylistCreatedNotification(p.id),
                        ]
                    })
            })
    }

    fn navigate(&self, dest: SidebarDestination) {
        let action = match dest {
            SidebarDestination::Library
            | SidebarDestination::SavedTracks
            | SidebarDestination::NowPlaying
            | SidebarDestination::SavedPlaylists => {
                BrowserAction::SetHomeVisiblePage(dest.id()).into()
            }
            SidebarDestination::Playlist(PlaylistSummary { id, .. }) => AppAction::ViewPlaylist(id),
        };
        self.dispatcher.dispatch(action);
    }
}

pub struct Sidebar {
    listbox: gtk::ListBox,
    list_store: gio::ListStore,
    model: Rc<SidebarModel>,
}

impl Sidebar {
    pub fn new(listbox: gtk::ListBox, model: Rc<SidebarModel>) -> Self {
        let popover = CreatePlaylistPopover::new();
        popover.connect_create(clone!(@weak model => move |t| model.create_new_playlist(t)));

        let list_store = gio::ListStore::new(SidebarItem::static_type());

        list_store.append(&SidebarItem::for_destination(SidebarDestination::Library));
        list_store.append(&SidebarItem::for_destination(
            SidebarDestination::SavedTracks,
        ));
        list_store.append(&SidebarItem::for_destination(
            SidebarDestination::NowPlaying,
        ));
        list_store.append(&SidebarItem::playlists_section());
        list_store.append(&SidebarItem::create_playlist_item());
        list_store.append(&SidebarItem::for_destination(
            SidebarDestination::SavedPlaylists,
        ));

        listbox.bind_model(
            Some(&list_store),
            clone!(@weak popover => @default-panic, move |obj| {
                let item = obj.downcast_ref::<SidebarItem>().unwrap();
                if item.navigatable() {
                    Self::make_navigatable(item)
                } else {
                    match item.id().as_str() {
                        SAVED_PLAYLISTS_SECTION => Self::make_section_label(item),
                        CREATE_PLAYLIST_ITEM => Self::make_create_playlist(item, popover),
                        _ => unimplemented!(),
                    }
                }
            }),
        );

        listbox.connect_row_activated(clone!(@weak popover, @weak model => move |_, row| {
            if let Some(row) = row.downcast_ref::<SidebarRow>() {
                if let Some(dest) = row.item().destination() {
                    model.navigate(dest);
                } else {
                    match row.item().id().as_str() {
                        CREATE_PLAYLIST_ITEM => popover.popup(),
                        _ => unimplemented!()
                    }
                }
            }
        }));

        Self {
            listbox,
            list_store,
            model,
        }
    }

    fn make_navigatable(item: &SidebarItem) -> gtk::Widget {
        let row = SidebarRow::new();
        row.set_selectable(false);
        row.set_item(item);
        row.upcast()
    }

    fn make_section_label(item: &SidebarItem) -> gtk::Widget {
        let label = gtk::Label::new(Some(item.title().as_str()));
        label.add_css_class("caption-heading");
        let row = gtk::ListBoxRow::builder()
            .activatable(false)
            .selectable(false)
            .sensitive(false)
            .child(&label)
            .build();
        row.upcast()
    }

    fn make_create_playlist(item: &SidebarItem, popover: CreatePlaylistPopover) -> gtk::Widget {
        let row = SidebarRow::new();
        row.set_activatable(true);
        row.set_selectable(false);
        row.set_sensitive(true);
        row.set_item(item);
        popover.set_parent(&row);
        row.upcast()
    }

    fn update_playlists_in_sidebar(&self) {
        let playlists: Vec<SidebarItem> = self
            .model
            .get_playlists()
            .into_iter()
            .map(SidebarItem::for_destination)
            .collect();
        self.list_store.splice(
            NUM_FIXED_ENTRIES,
            self.list_store.n_items() - NUM_FIXED_ENTRIES,
            playlists.as_slice(),
        );
    }
}

impl Component for Sidebar {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.listbox.upcast_ref()
    }
}

impl EventListener for Sidebar {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::BrowserEvent(BrowserEvent::SavedPlaylistsUpdated) = event {
            self.update_playlists_in_sidebar();
        }
    }
}
