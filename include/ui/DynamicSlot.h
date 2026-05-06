#pragma once

#include "ftxui/component/component_base.hpp"
#include "ftxui/component/event.hpp"

namespace mc_manager::ui {

class DynamicSlot : public ftxui::ComponentBase {
public:
    void set_child(ftxui::Component child) {
        if (!children_.empty()) {
            children_.clear();
        }
        if (child) {
            Add(std::move(child));
        }
    }

    ftxui::Element OnRender() override {
        if (children_.empty()) return ftxui::text("");
        return children_[0]->Render();
    }

    bool OnEvent(ftxui::Event event) override {
        if (children_.empty()) return false;
        return children_[0]->OnEvent(event);
    }

    bool Focusable() const override {
        if (children_.empty()) return false;
        return children_[0]->Focusable();
    }

    ftxui::Component ActiveChild() override {
        if (children_.empty()) return nullptr;
        return children_[0];
    }
};

} // namespace mc_manager::ui
