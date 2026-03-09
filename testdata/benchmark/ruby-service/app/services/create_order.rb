require_relative "../models/user"
require_relative "../models/order"

module Services
  class CreateOrder
    # interactor-like pattern: call as entry point
    def self.call(user_id:, items:)
      new(user_id, items).call
    end

    def initialize(user_id, items)
      @user_id = user_id
      @items = items
    end

    def call
      user = Models::User.find(@user_id)
      return { success: false, error: "User not found" } unless user

      order = Models::Order.new(user: user, items: @items)
      total = calculate_total(@items)
      order.total = total

      if order.save
        notify_user(user, order)
        { success: true, order: order }
      else
        { success: false, error: "Failed to save order" }
      end
    end

    private

    def calculate_total(items)
      items.sum { |item| item[:price] * item[:quantity] }
    end

    def notify_user(user, order)
      # notification logic
      user.last_order_at = Time.now
    end
  end
end
