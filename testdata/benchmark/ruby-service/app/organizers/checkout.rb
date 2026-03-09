require_relative "../services/create_order"
require_relative "../services/send_receipt"
require_relative "../services/update_inventory"

module Organizers
  class Checkout
    # interactor organizer pattern: chain of services
    STEPS = [
      Services::CreateOrder,
      Services::UpdateInventory,
      Services::SendReceipt,
    ]

    def self.call(user_id:, items:)
      new.call(user_id: user_id, items: items)
    end

    def call(user_id:, items:)
      context = { user_id: user_id, items: items }

      STEPS.each do |step|
        result = step.call(**context)
        unless result[:success]
          return { success: false, failed_step: step.name, error: result[:error] }
        end
        context.merge!(result)
      end

      context
    end
  end
end
