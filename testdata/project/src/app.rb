require_relative "user"
require "bcrypt"

class Application
  def initialize
    @user_service = UserService.new
  end

  def run
    user = @user_service.create("admin", "secret")
    puts user.full_name
  end
end
